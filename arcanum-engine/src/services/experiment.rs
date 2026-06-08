use arcanum_core::{Result, ArcanumError, types::{PerBackendChunkConfig, ExperimentId}};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExperimentStatus {
    Active,
    ReadyToPromote,
    Closed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExperimentMetrics {
    pub champion_recall_at_5:   f32,
    pub challenger_recall_at_5: f32,
    pub sample_size:            usize,
    pub computed_at:            String,  // ISO-8601
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShadowExperiment {
    pub id:                ExperimentId,
    pub challenger_config: PerBackendChunkConfig,
    pub started_at:        String,  // ISO-8601
    pub status:            ExperimentStatus,
    pub metrics:           Option<ExperimentMetrics>,
}

impl ShadowExperiment {
    /// Returns the shadow namespace name used in storage backends.
    pub fn shadow_namespace(&self, collection_id: &str) -> String {
        format!("{}__shadow_{}", collection_id, self.id.0)
    }
}

pub struct ExperimentService {
    experiments: Arc<RwLock<HashMap<String, ShadowExperiment>>>,  // key: "{col_id}:{exp_id}"
    collections: Arc<crate::services::collection::CollectionService>,
}

impl ExperimentService {
    pub fn new(collections: Arc<crate::services::collection::CollectionService>) -> Self {
        Self {
            experiments: Arc::new(RwLock::new(HashMap::new())),
            collections,
        }
    }

    pub async fn start(
        &self,
        collection_id: arcanum_core::types::CollectionId,
        challenger_config: PerBackendChunkConfig,
    ) -> Result<ShadowExperiment> {
        // Check no Active experiment already exists for this collection
        {
            let map = self.experiments.read().await;
            let has_active = map.iter().any(|(k, exp)| {
                k.starts_with(&format!("{}:", collection_id.0))
                    && exp.status == ExperimentStatus::Active
            });
            if has_active {
                return Err(ArcanumError::Storage(format!(
                    "collection '{}' already has an active experiment",
                    collection_id.0
                )));
            }
        }

        let exp = ShadowExperiment {
            id: ExperimentId::new(),
            challenger_config,
            started_at: chrono::Utc::now().to_rfc3339(),
            status: ExperimentStatus::Active,
            metrics: None,
        };

        let key = format!("{}:{}", collection_id.0, exp.id.0);
        self.experiments.write().await.insert(key, exp.clone());
        Ok(exp)
    }

    pub async fn get(&self, collection_id: &str, exp_id: &ExperimentId) -> Result<ShadowExperiment> {
        let key = format!("{}:{}", collection_id, exp_id.0);
        self.experiments.read().await.get(&key)
            .cloned()
            .ok_or_else(|| ArcanumError::NotFound(format!("experiment '{}'", exp_id.0)))
    }

    pub async fn promote(&self, collection_id: &str, exp_id: &ExperimentId) -> Result<()> {
        let key = format!("{}:{}", collection_id, exp_id.0);
        let challenger_config = {
            let map = self.experiments.read().await;
            let exp = map.get(&key)
                .ok_or_else(|| ArcanumError::NotFound(format!("experiment '{}'", exp_id.0)))?;
            exp.challenger_config.clone()
        };

        // Update collection's chunker_config
        self.collections.set_chunker_config(
            collection_id,
            Some(challenger_config),
        ).await?;

        // Close the experiment
        let mut map = self.experiments.write().await;
        if let Some(exp) = map.get_mut(&key) {
            exp.status = ExperimentStatus::Closed;
        }
        Ok(())
    }

    pub async fn abandon(&self, collection_id: &str, exp_id: &ExperimentId) -> Result<()> {
        let key = format!("{}:{}", collection_id, exp_id.0);
        let mut map = self.experiments.write().await;
        let exp = map.get_mut(&key)
            .ok_or_else(|| ArcanumError::NotFound(format!("experiment '{}'", exp_id.0)))?;
        exp.status = ExperimentStatus::Closed;
        Ok(())
    }

    pub async fn update_metrics(
        &self,
        collection_id: &str,
        exp_id: &ExperimentId,
        metrics: ExperimentMetrics,
    ) -> Result<ExperimentStatus> {
        let key = format!("{}:{}", collection_id, exp_id.0);
        let mut map = self.experiments.write().await;
        let exp = map.get_mut(&key)
            .ok_or_else(|| ArcanumError::NotFound(format!("experiment '{}'", exp_id.0)))?;

        exp.metrics = Some(metrics.clone());

        // Auto-flag as ready to promote when challenger leads by >=5% over >=50 docs
        if metrics.sample_size >= 50
            && metrics.challenger_recall_at_5 > metrics.champion_recall_at_5 + 0.05
        {
            exp.status = ExperimentStatus::ReadyToPromote;
        }

        Ok(exp.status.clone())
    }

    pub async fn active_experiments(&self) -> Vec<(String, ShadowExperiment)> {
        self.experiments.read().await
            .iter()
            .filter(|(_, exp)| exp.status == ExperimentStatus::Active)
            .map(|(k, exp)| {
                let col_id = k.split(':').next().unwrap_or("").to_string();
                (col_id, exp.clone())
            })
            .collect()
    }
}
