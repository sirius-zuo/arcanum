use arcanum_core::{
    traits::{ExperimentStatus, ExperimentStore, ShadowExperiment},
    types::ExperimentId,
    ArcanumError, Result,
};
use async_trait::async_trait;
use sqlx::PgPool;
use tracing::instrument;

/// PostgreSQL-backed ExperimentStore for production deployments.
///
/// Uses the `chunk_experiments` table (`migrations/0001_chunk_experiments.sql`), whose
/// partial unique index (`migrations/0002_chunk_experiments_active_unique.sql`) makes
/// `try_start` race-free across processes.
pub struct PostgresExperimentStore {
    pool: PgPool,
}

impl PostgresExperimentStore {
    pub async fn new(database_url: &str) -> Result<Self> {
        let pool = PgPool::connect(database_url).await
            .map_err(|e| ArcanumError::Storage(format!("PostgresExperimentStore connect: {}", e)))?;
        let store = Self { pool };
        store.ensure_schema().await?;
        Ok(store)
    }

    async fn ensure_schema(&self) -> Result<()> {
        sqlx::query(r#"
            CREATE TABLE IF NOT EXISTS chunk_experiments (
                id                UUID        NOT NULL PRIMARY KEY,
                collection_id     TEXT        NOT NULL,
                status            TEXT        NOT NULL
                                    CHECK (status IN ('active', 'ready_to_promote', 'closed')),
                challenger_config JSONB       NOT NULL,
                metrics           JSONB,
                started_at        TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                closed_at         TIMESTAMPTZ
            )
        "#).execute(&self.pool).await
            .map_err(|e| ArcanumError::Storage(format!("ensure chunk_experiments: {}", e)))?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_chunk_experiments_collection_status ON chunk_experiments (collection_id, status)")
            .execute(&self.pool).await
            .map_err(|e| ArcanumError::Storage(format!("ensure collection_status index: {}", e)))?;

        // Enforces one Active experiment per collection at the database level, so
        // try_start is race-free across processes/connections (migrations/0002).
        sqlx::query(r#"
            CREATE UNIQUE INDEX IF NOT EXISTS idx_chunk_experiments_one_active
                ON chunk_experiments (collection_id)
                WHERE status = 'active'
        "#).execute(&self.pool).await
            .map_err(|e| ArcanumError::Storage(format!("ensure one-active index: {}", e)))?;

        Ok(())
    }
}

#[derive(sqlx::FromRow)]
struct ExperimentRow {
    id:                uuid::Uuid,
    status:            String,
    challenger_config: serde_json::Value,
    metrics:           Option<serde_json::Value>,
    started_at:        chrono::DateTime<chrono::Utc>,
}

impl ExperimentRow {
    fn into_experiment(self) -> Result<ShadowExperiment> {
        Ok(ShadowExperiment {
            id: ExperimentId(self.id),
            challenger_config: serde_json::from_value(self.challenger_config)
                .map_err(|e| ArcanumError::Storage(format!("deserialize challenger_config: {}", e)))?,
            started_at: self.started_at.to_rfc3339(),
            status: status_from_str(&self.status)?,
            metrics: self.metrics.map(serde_json::from_value)
                .transpose()
                .map_err(|e| ArcanumError::Storage(format!("deserialize metrics: {}", e)))?,
        })
    }
}

/// Round-trips `ExperimentStatus` through its existing `#[serde(rename_all = "snake_case")]`
/// instead of hand-matching each variant against the DB's TEXT encoding.
fn status_to_str(status: &ExperimentStatus) -> Result<String> {
    match serde_json::to_value(status)
        .map_err(|e| ArcanumError::Storage(format!("serialize status: {}", e)))?
    {
        serde_json::Value::String(s) => Ok(s),
        other => Err(ArcanumError::Storage(format!("unexpected status serialization: {}", other))),
    }
}

fn status_from_str(s: &str) -> Result<ExperimentStatus> {
    serde_json::from_value(serde_json::Value::String(s.to_string()))
        .map_err(|e| ArcanumError::Storage(format!("unknown experiment status '{}': {}", s, e)))
}

#[async_trait]
impl ExperimentStore for PostgresExperimentStore {
    #[instrument(skip(self, exp), fields(store = "postgres_experiment", collection_id, exp_id = %exp.id.0), err)]
    async fn try_start(&self, collection_id: &str, exp: &ShadowExperiment) -> Result<()> {
        let res = sqlx::query(
            r#"INSERT INTO chunk_experiments (id, collection_id, status, challenger_config, started_at)
               VALUES ($1, $2, 'active', $3, $4::timestamptz)"#)
            .bind(exp.id.0)
            .bind(collection_id)
            .bind(serde_json::to_value(&exp.challenger_config)
                .map_err(|e| ArcanumError::Storage(format!("challenger_config serialize: {}", e)))?)
            .bind(&exp.started_at)
            .execute(&self.pool).await;
        match res {
            Ok(_) => Ok(()),
            Err(sqlx::Error::Database(db)) if db.constraint() == Some("idx_chunk_experiments_one_active") =>
                Err(ArcanumError::Storage(format!(
                    "collection '{}' already has an active experiment", collection_id))),
            Err(e) => Err(ArcanumError::Storage(format!("experiment insert: {}", e))),
        }
    }

    #[instrument(skip(self), fields(store = "postgres_experiment", collection_id, exp_id = %exp_id.0), err)]
    async fn get(&self, collection_id: &str, exp_id: &ExperimentId) -> Result<Option<ShadowExperiment>> {
        let row = sqlx::query_as::<_, ExperimentRow>(
            r#"SELECT id, status, challenger_config, metrics, started_at
               FROM chunk_experiments WHERE id = $1 AND collection_id = $2"#)
            .bind(exp_id.0)
            .bind(collection_id)
            .fetch_optional(&self.pool).await
            .map_err(|e| ArcanumError::Storage(format!("get experiment: {}", e)))?;

        row.map(ExperimentRow::into_experiment).transpose()
    }

    #[instrument(skip(self, exp), fields(store = "postgres_experiment", collection_id, exp_id = %exp.id.0), err)]
    async fn update(&self, collection_id: &str, exp: &ShadowExperiment) -> Result<()> {
        let status = status_to_str(&exp.status)?;
        // Race guard (carried from Task 2's review): the service does get-then-update
        // (read snapshot, mutate, write), so a blind full-row UPDATE could clobber a
        // concurrent close — e.g. a stale update_metrics writing status='active' over a
        // just-landed 'closed', resurrecting a closed experiment and defeating the
        // service's closed-guard. Excluding already-closed rows from the WHERE clause
        // makes that race a no-op update (rows_affected == 0) instead of a silent
        // overwrite; every legitimate transition (->closed, or update while non-closed)
        // is unaffected.
        let done = sqlx::query(
            r#"UPDATE chunk_experiments
               SET status = $3, metrics = $4,
                   closed_at = CASE WHEN $3 = 'closed' THEN NOW() ELSE closed_at END
               WHERE id = $1 AND collection_id = $2 AND status != 'closed'"#)
            .bind(exp.id.0)
            .bind(collection_id)
            .bind(&status)
            .bind(exp.metrics.as_ref().map(serde_json::to_value).transpose()
                .map_err(|e| ArcanumError::Storage(format!("metrics serialize: {}", e)))?)
            .execute(&self.pool).await
            .map_err(|e| ArcanumError::Storage(format!("experiment update: {}", e)))?;
        if done.rows_affected() == 0 {
            return Err(ArcanumError::NotFound(format!(
                "experiment '{}' (missing or already closed)", exp.id.0)));
        }
        Ok(())
    }

    #[instrument(skip(self), fields(store = "postgres_experiment"), err)]
    async fn active_experiments(&self) -> Result<Vec<(String, ShadowExperiment)>> {
        #[derive(sqlx::FromRow)]
        struct ActiveRow {
            collection_id: String,
            #[sqlx(flatten)]
            experiment: ExperimentRow,
        }

        let rows = sqlx::query_as::<_, ActiveRow>(
            r#"SELECT collection_id, id, status, challenger_config, metrics, started_at
               FROM chunk_experiments WHERE status = 'active'"#)
            .fetch_all(&self.pool).await
            .map_err(|e| ArcanumError::Storage(format!("active_experiments: {}", e)))?;

        rows.into_iter()
            .map(|r| Ok((r.collection_id, r.experiment.into_experiment()?)))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arcanum_core::traits::ExperimentMetrics;
    use arcanum_core::types::{ChunkStrategyConfig, ExperimentId, PerBackendChunkConfig};

    fn sample_exp() -> ShadowExperiment {
        ShadowExperiment {
            id: ExperimentId::new(),
            challenger_config: PerBackendChunkConfig {
                vector: ChunkStrategyConfig {
                    strategy: "fixed".to_string(),
                    params: serde_json::json!({ "chunk_size": 512, "overlap": 64 }),
                },
                graph: None,
                tree: None,
            },
            started_at: chrono::Utc::now().to_rfc3339(),
            status: ExperimentStatus::Active,
            metrics: None,
        }
    }

    async fn store() -> PostgresExperimentStore {
        let url = std::env::var("TEST_DATABASE_URL")
            .expect("TEST_DATABASE_URL must be set for Postgres experiment store tests");
        PostgresExperimentStore::new(&url).await.expect("connect")
    }

    fn unique_collection(prefix: &str) -> String {
        format!("{}-{}", prefix, uuid::Uuid::new_v4())
    }

    // Adapted verbatim (semantics) from arcanum-core/src/traits/experiment.rs's
    // InMemoryExperimentStore contract tests, against the Postgres-backed store.

    #[tokio::test]
    #[ignore = "requires Postgres — set TEST_DATABASE_URL"]
    async fn try_start_rejects_second_active_for_same_collection() {
        let store = store().await;
        let col1 = unique_collection("try-start-rejects-col1");
        let col2 = unique_collection("try-start-rejects-col2");

        store.try_start(&col1, &sample_exp()).await.unwrap();
        assert!(store.try_start(&col1, &sample_exp()).await.is_err());
        store.try_start(&col2, &sample_exp()).await.unwrap(); // other collections unaffected
    }

    #[tokio::test]
    #[ignore = "requires Postgres — set TEST_DATABASE_URL"]
    async fn closed_experiment_frees_the_active_slot() {
        let store = store().await;
        let col1 = unique_collection("closed-frees-slot-col1");

        let mut exp = sample_exp();
        store.try_start(&col1, &exp).await.unwrap();
        exp.status = ExperimentStatus::Closed;
        store.update(&col1, &exp).await.unwrap();
        assert!(store.try_start(&col1, &sample_exp()).await.is_ok());
    }

    #[tokio::test]
    #[ignore = "requires Postgres — set TEST_DATABASE_URL"]
    async fn get_update_roundtrip_and_active_listing() {
        let store = store().await;
        let col1 = unique_collection("roundtrip-col1");

        let exp = sample_exp();
        store.try_start(&col1, &exp).await.unwrap();
        assert_eq!(store.get(&col1, &exp.id).await.unwrap().unwrap().id, exp.id);
        assert!(store.get(&col1, &ExperimentId::new()).await.unwrap().is_none());
        let active = store.active_experiments().await.unwrap();
        assert!(active.contains(&(col1.clone(), exp.clone())));
    }

    #[tokio::test]
    #[ignore = "requires Postgres — set TEST_DATABASE_URL"]
    async fn concurrent_try_start_exactly_one_succeeds() {
        let store = store().await;
        let col1 = unique_collection("concurrent-try-start-col1");

        let (exp1, exp2) = (sample_exp(), sample_exp());
        let (r1, r2) = tokio::join!(
            store.try_start(&col1, &exp1),
            store.try_start(&col1, &exp2),
        );
        let successes = [&r1, &r2].iter().filter(|r| r.is_ok()).count();
        assert_eq!(successes, 1, "exactly one concurrent try_start must succeed: {:?} / {:?}", r1, r2);
    }

    #[tokio::test]
    #[ignore = "requires Postgres — set TEST_DATABASE_URL"]
    async fn update_rejects_a_stale_write_over_an_already_closed_experiment() {
        // Race guard carried from Task 2's review: a get-then-update on a closed
        // experiment must not resurrect it (e.g. a stale update_metrics landing
        // after a concurrent promote/abandon already closed it).
        let store = store().await;
        let col1 = unique_collection("stale-write-guard-col1");

        let mut exp = sample_exp();
        store.try_start(&col1, &exp).await.unwrap();

        let mut closed = exp.clone();
        closed.status = ExperimentStatus::Closed;
        store.update(&col1, &closed).await.unwrap();

        // Stale update racing in after the close — must be rejected, not resurrect the row.
        exp.status = ExperimentStatus::Active;
        exp.metrics = Some(ExperimentMetrics {
            champion_recall_at_5: 0.5,
            challenger_recall_at_5: 0.6,
            sample_size: 100,
            computed_at: chrono::Utc::now().to_rfc3339(),
        });
        let err = store.update(&col1, &exp).await.unwrap_err();
        assert!(matches!(err, ArcanumError::NotFound(_)));

        // The row must still read back as closed.
        let fetched = store.get(&col1, &exp.id).await.unwrap().unwrap();
        assert_eq!(fetched.status, ExperimentStatus::Closed);
    }
}
