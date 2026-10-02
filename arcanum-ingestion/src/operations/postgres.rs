use arcanum_core::{
    traits::OperationStore,
    types::{
        CollectionId, CreateOperationResult, IngestionOperation, IngestionReport,
        IngestionSubmission, OperationId, OperationStatus,
    },
    ArcanumError, Result,
};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use tracing::instrument;

use super::{parse_status, status_str, submission_hash, validate_transition};

/// PostgreSQL-backed `OperationStore` for production deployments.
///
/// Uses an `ingestion_operations` table with a UNIQUE `idempotency_key` so
/// duplicate submissions return the original operation instead of enqueueing
/// duplicate work, and a CHECK constraint on `status` so only the four
/// canonical statuses are stored.
pub struct PostgresOperationStore {
    pool: PgPool,
}

impl PostgresOperationStore {
    pub async fn new(database_url: &str) -> Result<Self> {
        let pool = PgPool::connect(database_url)
            .await
            .map_err(|e| ArcanumError::Storage(format!("PostgresOperationStore connect: {e}")))?;
        let store = Self { pool };
        store.ensure_schema().await?;
        Ok(store)
    }

    async fn ensure_schema(&self) -> Result<()> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS ingestion_operations (
                operation_id            UUID        PRIMARY KEY,
                idempotency_key         TEXT        NOT NULL UNIQUE,
                submission_hash         TEXT        NOT NULL,
                status                  TEXT        NOT NULL
                                            CHECK (status IN ('accepted','running','succeeded','failed')),
                logical_source_uri      TEXT        NOT NULL,
                mime_hint               TEXT,
                collection_id           TEXT        NOT NULL,
                pipeline_configuration  JSONB       NOT NULL,
                payload_locator         TEXT,
                accepted_at             TIMESTAMPTZ NOT NULL,
                started_at              TIMESTAMPTZ,
                terminal_report         JSONB
            )
            "#,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| ArcanumError::Storage(format!("ensure ingestion_operations: {e}")))?;
        Ok(())
    }
}

#[derive(sqlx::FromRow)]
struct OperationRow {
    operation_id: uuid::Uuid,
    idempotency_key: String,
    status: String,
    logical_source_uri: String,
    mime_hint: Option<String>,
    collection_id: String,
    pipeline_configuration: serde_json::Value,
    payload_locator: Option<String>,
    accepted_at: DateTime<Utc>,
    started_at: Option<DateTime<Utc>>,
    terminal_report: Option<serde_json::Value>,
}

fn row_to_operation(r: OperationRow) -> Result<IngestionOperation> {
    let terminal_report = r
        .terminal_report
        .map(|v| {
            serde_json::from_value(v)
                .map_err(|e| ArcanumError::Storage(format!("deserialize terminal report: {e}")))
        })
        .transpose()?;
    Ok(IngestionOperation {
        operation_id: OperationId(r.operation_id),
        submission: IngestionSubmission {
            idempotency_key: r.idempotency_key,
            logical_source_uri: r.logical_source_uri,
            mime_hint: r.mime_hint,
            collection_id: CollectionId(r.collection_id),
            pipeline_configuration: r.pipeline_configuration,
            payload: None,
            payload_locator: r.payload_locator,
        },
        status: parse_status(&r.status)?,
        accepted_at: r.accepted_at,
        started_at: r.started_at,
        terminal_report,
    })
}

#[async_trait]
impl OperationStore for PostgresOperationStore {
    #[instrument(skip(self, submission), fields(store = "postgres_operation", idem = submission.idempotency_key), err)]
    async fn create_or_get(
        &self,
        submission: &IngestionSubmission,
    ) -> Result<CreateOperationResult> {
        let operation_id = OperationId::new();
        let hash = submission_hash(submission)?;
        let accepted_at = Utc::now();

        let inserted = sqlx::query(
            r#"
            INSERT INTO ingestion_operations
               (operation_id, idempotency_key, submission_hash, status,
                logical_source_uri, mime_hint, collection_id, pipeline_configuration,
                payload_locator, accepted_at)
               VALUES ($1, $2, $3, 'accepted', $4, $5, $6, $7, $8, $9)
               ON CONFLICT (idempotency_key) DO NOTHING
            "#,
        )
        .bind(operation_id.0)
        .bind(&submission.idempotency_key)
        .bind(&hash)
        .bind(&submission.logical_source_uri)
        .bind(&submission.mime_hint)
        .bind(&submission.collection_id.0)
        .bind(&submission.pipeline_configuration)
        .bind(&submission.payload_locator)
        .bind(accepted_at)
        .execute(&self.pool)
        .await
        .map_err(|e| ArcanumError::Storage(format!("create operation: {e}")))?;

        if inserted.rows_affected() == 1 {
            return Ok(CreateOperationResult {
                operation: IngestionOperation {
                    operation_id,
                    submission: submission.clone(),
                    status: OperationStatus::Accepted,
                    accepted_at,
                    started_at: None,
                    terminal_report: None,
                },
                is_new: true,
            });
        }

        // Existing row for this idempotency key. Verify the submission is identical.
        let (stored_hash,): (String,) = sqlx::query_as(
            "SELECT submission_hash FROM ingestion_operations WHERE idempotency_key = $1",
        )
        .bind(&submission.idempotency_key)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| ArcanumError::Storage(format!("fetch submission hash: {e}")))?;

        if stored_hash == hash {
            let existing = self
                .get_by_idempotency(&submission.idempotency_key)
                .await?
                .ok_or_else(|| ArcanumError::Storage("idempotency key row vanished".into()))?;
            Ok(CreateOperationResult {
                operation: existing,
                is_new: false,
            })
        } else {
            Err(ArcanumError::Conflict(format!(
                "idempotency key {} already used by a different submission",
                submission.idempotency_key
            )))
        }
    }

    #[instrument(skip(self), fields(store = "postgres_operation", op_id = %id.0), err)]
    async fn mark_running(&self, id: &OperationId, started_at: DateTime<Utc>) -> Result<()> {
        let updated = sqlx::query(
            r#"
            UPDATE ingestion_operations
               SET status = 'running', started_at = $2
             WHERE operation_id = $1 AND status = 'accepted'
            "#,
        )
        .bind(id.0)
        .bind(started_at)
        .execute(&self.pool)
        .await
        .map_err(|e| ArcanumError::Storage(format!("mark running: {e}")))?;

        if updated.rows_affected() == 1 {
            return Ok(());
        }
        let current = self
            .get(id)
            .await?
            .ok_or_else(|| ArcanumError::NotFound(format!("operation not found: {}", id.0)))?;
        Err(ArcanumError::Conflict(format!(
            "cannot mark running: operation {} is {:?}",
            id.0, current.status
        )))
    }

    #[instrument(skip(self, report), fields(store = "postgres_operation", op_id = %report.operation_id.0), err)]
    async fn complete(&self, report: &IngestionReport) -> Result<()> {
        let current = self.get(&report.operation_id).await?.ok_or_else(|| {
            ArcanumError::NotFound(format!("operation not found: {}", report.operation_id.0))
        })?;
        validate_transition(&current, report)?;

        let report_value = serde_json::to_value(report)
            .map_err(|e| ArcanumError::Storage(format!("serialize terminal report: {e}")))?;
        sqlx::query(
            r#"
            UPDATE ingestion_operations
               SET status = $2, terminal_report = $3
             WHERE operation_id = $1
            "#,
        )
        .bind(report.operation_id.0)
        .bind(status_str(&report.status))
        .bind(&report_value)
        .execute(&self.pool)
        .await
        .map_err(|e| ArcanumError::Storage(format!("complete operation: {e}")))?;
        Ok(())
    }

    async fn get(&self, id: &OperationId) -> Result<Option<IngestionOperation>> {
        let row = sqlx::query_as::<_, OperationRow>(
            "SELECT operation_id, idempotency_key, status, logical_source_uri, \
             mime_hint, collection_id, pipeline_configuration, payload_locator, accepted_at, \
             started_at, terminal_report FROM ingestion_operations WHERE operation_id = $1",
        )
        .bind(id.0)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| ArcanumError::Storage(format!("get operation: {e}")))?;
        row.map(row_to_operation).transpose()
    }

    async fn get_by_idempotency(&self, key: &str) -> Result<Option<IngestionOperation>> {
        let row = sqlx::query_as::<_, OperationRow>(
            "SELECT operation_id, idempotency_key, status, logical_source_uri, \
             mime_hint, collection_id, pipeline_configuration, payload_locator, accepted_at, \
             started_at, terminal_report FROM ingestion_operations WHERE idempotency_key = $1",
        )
        .bind(key)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| ArcanumError::Storage(format!("get by idempotency: {e}")))?;
        row.map(row_to_operation).transpose()
    }
}
