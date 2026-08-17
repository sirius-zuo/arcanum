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
use sqlx::SqlitePool;
use tracing::instrument;

use super::{parse_status, status_str, submission_hash, validate_transition};

/// SQLite-backed `OperationStore` for local development and tests.
/// Use `PostgresOperationStore` in production.
///
/// `create_or_get` runs in an immediate transaction so the idempotency-key
/// check and insert are atomic against concurrent writers.
pub struct SqliteOperationStore {
    pool: SqlitePool,
}

impl SqliteOperationStore {
    /// Open (or create) a SQLite operation store at `path`.
    /// Use `":memory:"` for in-process tests.
    pub async fn open(path: &str) -> Result<Self> {
        if path != ":memory:" {
            if let Some(parent) = std::path::Path::new(path).parent() {
                if !parent.as_os_str().is_empty() {
                    tokio::fs::create_dir_all(parent)
                        .await
                        .map_err(|e| ArcanumError::Storage(format!("create db dir: {e}")))?;
                }
            }
        }
        let url = if path == ":memory:" {
            "sqlite::memory:".to_string()
        } else if path.starts_with("sqlite:") {
            path.to_string()
        } else {
            format!("sqlite://{}?mode=rwc", path)
        };
        let pool = SqlitePool::connect(&url)
            .await
            .map_err(|e| ArcanumError::Storage(format!("SqliteOperationStore open: {e}")))?;
        let store = Self { pool };
        store.ensure_schema().await?;
        Ok(store)
    }

    async fn ensure_schema(&self) -> Result<()> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS ingestion_operations (
                operation_id            TEXT    NOT NULL PRIMARY KEY,
                idempotency_key         TEXT    NOT NULL UNIQUE,
                submission_hash         TEXT    NOT NULL,
                status                  TEXT    NOT NULL
                                            CHECK (status IN ('accepted','running','succeeded','failed')),
                logical_source_uri      TEXT    NOT NULL,
                mime_hint               TEXT,
                collection_id           TEXT    NOT NULL,
                pipeline_configuration  TEXT    NOT NULL,
                payload_locator         TEXT,
                accepted_at             TEXT    NOT NULL,
                started_at              TEXT,
                terminal_report         TEXT
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
    operation_id: String,
    idempotency_key: String,
    status: String,
    logical_source_uri: String,
    mime_hint: Option<String>,
    collection_id: String,
    pipeline_configuration: String,
    payload_locator: Option<String>,
    accepted_at: DateTime<Utc>,
    started_at: Option<DateTime<Utc>>,
    terminal_report: Option<String>,
}

fn row_to_operation(r: OperationRow) -> Result<IngestionOperation> {
    let pipeline_configuration: serde_json::Value = serde_json::from_str(&r.pipeline_configuration)
        .map_err(|e| ArcanumError::Storage(format!("deserialize pipeline configuration: {e}")))?;
    let terminal_report = r
        .terminal_report
        .map(|s| {
            serde_json::from_str(&s)
                .map_err(|e| ArcanumError::Storage(format!("deserialize terminal report: {e}")))
        })
        .transpose()?;
    let operation_id = uuid::Uuid::parse_str(&r.operation_id)
        .map_err(|e| ArcanumError::Storage(format!("invalid operation_id uuid: {e}")))?;
    Ok(IngestionOperation {
        operation_id: OperationId(operation_id),
        submission: IngestionSubmission {
            idempotency_key: r.idempotency_key,
            logical_source_uri: r.logical_source_uri,
            mime_hint: r.mime_hint,
            collection_id: CollectionId(r.collection_id),
            pipeline_configuration,
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
impl OperationStore for SqliteOperationStore {
    #[instrument(skip(self, submission), fields(store = "sqlite_operation", idem = submission.idempotency_key), err)]
    async fn create_or_get(
        &self,
        submission: &IngestionSubmission,
    ) -> Result<CreateOperationResult> {
        let operation_id = OperationId::new();
        let hash = submission_hash(submission)?;
        let accepted_at = Utc::now();
        let pipeline_json = serde_json::to_string(&submission.pipeline_configuration)
            .map_err(|e| ArcanumError::Storage(format!("serialize pipeline configuration: {e}")))?;

        // Immediate transaction: the idempotency-key check + insert are atomic
        // and the write lock is taken up front, avoiding SQLITE_BUSY upgrade
        // deadlocks between concurrent writers.
        let mut conn = self
            .pool
            .acquire()
            .await
            .map_err(|e| ArcanumError::Storage(format!("acquire sqlite connection: {e}")))?;
        sqlx::query("BEGIN IMMEDIATE")
            .execute(&mut *conn)
            .await
            .map_err(|e| ArcanumError::Storage(format!("begin immediate: {e}")))?;

        enum Outcome {
            Inserted {
                operation_id: OperationId,
                accepted_at: DateTime<Utc>,
            },
            Existing {
                stored_hash: String,
            },
        }
        let outcome: std::result::Result<Outcome, ArcanumError> = async {
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
            .bind(operation_id.0.to_string())
            .bind(&submission.idempotency_key)
            .bind(&hash)
            .bind(&submission.logical_source_uri)
            .bind(&submission.mime_hint)
            .bind(&submission.collection_id.0)
            .bind(&pipeline_json)
            .bind(&submission.payload_locator)
            .bind(accepted_at)
            .execute(&mut *conn)
            .await
            .map_err(|e| ArcanumError::Storage(format!("create operation: {e}")))?;

            if inserted.rows_affected() == 1 {
                return Ok(Outcome::Inserted {
                    operation_id,
                    accepted_at,
                });
            }

            let (stored_hash,): (String,) = sqlx::query_as(
                "SELECT submission_hash FROM ingestion_operations WHERE idempotency_key = $1",
            )
            .bind(&submission.idempotency_key)
            .fetch_one(&mut *conn)
            .await
            .map_err(|e| ArcanumError::Storage(format!("fetch submission hash: {e}")))?;
            Ok(Outcome::Existing { stored_hash })
        }
        .await;

        match outcome {
            Ok(Outcome::Inserted {
                operation_id,
                accepted_at,
            }) => {
                sqlx::query("COMMIT")
                    .execute(&mut *conn)
                    .await
                    .map_err(|e| ArcanumError::Storage(format!("commit create: {e}")))?;
                Ok(CreateOperationResult {
                    operation: IngestionOperation {
                        operation_id,
                        submission: submission.clone(),
                        status: OperationStatus::Accepted,
                        accepted_at,
                        started_at: None,
                        terminal_report: None,
                    },
                    is_new: true,
                })
            }
            Ok(Outcome::Existing { stored_hash }) => {
                sqlx::query("COMMIT")
                    .execute(&mut *conn)
                    .await
                    .map_err(|e| ArcanumError::Storage(format!("commit lookup: {e}")))?;
                if stored_hash == hash {
                    let existing = self
                        .get_by_idempotency(&submission.idempotency_key)
                        .await?
                        .ok_or_else(|| {
                            ArcanumError::Storage("idempotency key row vanished".into())
                        })?;
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
            Err(e) => {
                let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
                Err(e)
            }
        }
    }

    #[instrument(skip(self), fields(store = "sqlite_operation", op_id = %id.0), err)]
    async fn mark_running(&self, id: &OperationId, started_at: DateTime<Utc>) -> Result<()> {
        let updated = sqlx::query(
            r#"
            UPDATE ingestion_operations
               SET status = 'running', started_at = $2
             WHERE operation_id = $1 AND status = 'accepted'
            "#,
        )
        .bind(id.0.to_string())
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

    #[instrument(skip(self, report), fields(store = "sqlite_operation", op_id = %report.operation_id.0), err)]
    async fn complete(&self, report: &IngestionReport) -> Result<()> {
        let current = self.get(&report.operation_id).await?.ok_or_else(|| {
            ArcanumError::NotFound(format!("operation not found: {}", report.operation_id.0))
        })?;
        validate_transition(&current, report)?;

        let report_json = serde_json::to_string(report)
            .map_err(|e| ArcanumError::Storage(format!("serialize terminal report: {e}")))?;
        sqlx::query(
            r#"
            UPDATE ingestion_operations
               SET status = $2, terminal_report = $3
             WHERE operation_id = $1
            "#,
        )
        .bind(report.operation_id.0.to_string())
        .bind(status_str(&report.status))
        .bind(&report_json)
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
        .bind(id.0.to_string())
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
