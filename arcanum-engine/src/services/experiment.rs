use arcanum_core::{Result, ArcanumError, types::{PerBackendChunkConfig, ExperimentId}};
use std::sync::Arc;

pub use arcanum_core::traits::{
    ExperimentStatus, ExperimentMetrics, ShadowExperiment, ExperimentStore, InMemoryExperimentStore,
};

pub struct ExperimentService {
    store: Arc<dyn ExperimentStore>,
    collections: Arc<crate::services::collection::CollectionService>,
}

impl ExperimentService {
    pub fn new(
        collections: Arc<crate::services::collection::CollectionService>,
        store: Arc<dyn ExperimentStore>,
    ) -> Self {
        Self { store, collections }
    }

    pub async fn start(
        &self,
        collection_id: arcanum_core::types::CollectionId,
        challenger_config: PerBackendChunkConfig,
    ) -> Result<ShadowExperiment> {
        let exp = ShadowExperiment {
            id: ExperimentId::new(),
            challenger_config,
            started_at: chrono::Utc::now().to_rfc3339(),
            status: ExperimentStatus::Active,
            metrics: None,
        };

        // Atomic check-then-insert lives in the store (preserves TOCTOU fix, finding #7).
        self.store.try_start(&collection_id.0, &exp).await?;

        // Link experiment to collection so the per-job resolver can find it.
        self.collections.set_experiment(&collection_id.0, Some(exp.id.clone())).await?;

        Ok(exp)
    }

    pub async fn get(&self, collection_id: &str, exp_id: &ExperimentId) -> Result<ShadowExperiment> {
        self.store.get(collection_id, exp_id).await?
            .ok_or_else(|| ArcanumError::NotFound(format!("experiment '{}'", exp_id.0)))
    }

    pub async fn promote(&self, collection_id: &str, exp_id: &ExperimentId) -> Result<()> {
        let mut exp = self.get(collection_id, exp_id).await?;

        // Guard: must be ReadyToPromote — prevents promoting an untested experiment (finding #6).
        if exp.status != ExperimentStatus::ReadyToPromote {
            return Err(ArcanumError::Storage(format!(
                "experiment '{}' cannot be promoted from status {:?}; status must be ReadyToPromote",
                exp_id.0, exp.status
            )));
        }

        self.collections.set_chunker_config(collection_id, Some(exp.challenger_config.clone())).await?;

        // Clear the experiment link on the collection before closing the experiment (finding #10).
        self.collections.set_experiment(collection_id, None).await?;

        exp.status = ExperimentStatus::Closed;
        self.store.update(collection_id, &exp).await?;
        Ok(())
    }

    pub async fn abandon(&self, collection_id: &str, exp_id: &ExperimentId) -> Result<()> {
        // Clear the experiment link on the collection first.
        self.collections.set_experiment(collection_id, None).await?;

        let mut exp = self.get(collection_id, exp_id).await?;
        exp.status = ExperimentStatus::Closed;
        self.store.update(collection_id, &exp).await?;
        Ok(())
    }

    pub async fn update_metrics(
        &self,
        collection_id: &str,
        exp_id: &ExperimentId,
        metrics: ExperimentMetrics,
    ) -> Result<ExperimentStatus> {
        let mut exp = self.get(collection_id, exp_id).await?;

        // Guard: a closed experiment must not be re-opened by a delayed eval run (finding #9).
        if exp.status == ExperimentStatus::Closed {
            return Err(ArcanumError::Storage(format!(
                "experiment '{}' is already closed and cannot receive metric updates",
                exp_id.0
            )));
        }

        exp.metrics = Some(metrics.clone());

        if metrics.sample_size >= 50
            && metrics.challenger_recall_at_5 > metrics.champion_recall_at_5 + 0.05
        {
            exp.status = ExperimentStatus::ReadyToPromote;
        }

        self.store.update(collection_id, &exp).await?;
        Ok(exp.status)
    }

    pub async fn active_experiments(&self) -> Vec<(String, ShadowExperiment)> {
        self.store.active_experiments().await.unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arcanum_core::types::CollectionId;

    fn fixed_config(size: u64) -> PerBackendChunkConfig {
        PerBackendChunkConfig {
            vector: arcanum_core::types::ChunkStrategyConfig {
                strategy: "fixed".to_string(),
                params: serde_json::json!({ "chunk_size": size, "overlap": 8 }),
            },
            graph: None,
            tree: None,
        }
    }

    fn mock_collection_service() -> Arc<crate::services::collection::CollectionService> {
        Arc::new(crate::services::collection::CollectionService::new(
            arcanum_core::config::ArcanumConfig::default(),
            Arc::new(crate::audit::AuditLogger::new()),
            Arc::new(crate::auth::AuthMiddleware::new("a-32-char-secret-for-testing-ok!")),
            Arc::new(arcanum_ingestion::PreprocessorCatalog::new()),
        ))
    }

    // Proves reads go through the store (not a private map): pre-load an experiment
    // directly via the store's try_start, then confirm the service sees it via get().
    #[tokio::test]
    async fn service_reads_experiment_preloaded_directly_into_the_store() {
        let store: Arc<dyn ExperimentStore> = Arc::new(InMemoryExperimentStore::new());
        let collections = mock_collection_service();
        let col_id = CollectionId("preloaded-col".into());
        let claims = crate::auth::ApiKeyClaims {
            user_id: "test".into(), allowed_collections: vec![], is_admin: true, exp: 9999999999,
        };
        collections.create(col_id.clone(), "test".into(), &claims).await.unwrap();

        let exp = ShadowExperiment {
            id: ExperimentId::new(),
            challenger_config: fixed_config(256),
            started_at: chrono::Utc::now().to_rfc3339(),
            status: ExperimentStatus::Active,
            metrics: None,
        };
        store.try_start(&col_id.0, &exp).await.unwrap();

        let svc = ExperimentService::new(collections, store);
        let fetched = svc.get(&col_id.0, &exp.id).await.unwrap();
        assert_eq!(fetched.id, exp.id);
        assert_eq!(fetched.status, ExperimentStatus::Active);
    }
}
