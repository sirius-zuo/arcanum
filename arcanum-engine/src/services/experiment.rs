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
        let exp = ShadowExperiment {
            id: ExperimentId::new(),
            challenger_config,
            started_at: chrono::Utc::now().to_rfc3339(),
            status: ExperimentStatus::Active,
            metrics: None,
        };
        let key = format!("{}:{}", collection_id.0, exp.id.0);

        // Single write lock for atomic check-then-insert — eliminates TOCTOU race (finding #7).
        {
            let mut map = self.experiments.write().await;
            let has_active = map.iter().any(|(k, e)| {
                k.starts_with(&format!("{}:", collection_id.0))
                    && e.status == ExperimentStatus::Active
            });
            if has_active {
                return Err(ArcanumError::Storage(format!(
                    "collection '{}' already has an active experiment",
                    collection_id.0
                )));
            }
            map.insert(key, exp.clone());
        }

        // Link experiment to collection so the per-job resolver can find it.
        self.collections.set_experiment(&collection_id.0, Some(exp.id.clone())).await?;

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
            // Guard: must be ReadyToPromote — prevents promoting an untested experiment (finding #6).
            if exp.status != ExperimentStatus::ReadyToPromote {
                return Err(ArcanumError::Storage(format!(
                    "experiment '{}' cannot be promoted from status {:?}; status must be ReadyToPromote",
                    exp_id.0, exp.status
                )));
            }
            exp.challenger_config.clone()
        };

        self.collections.set_chunker_config(collection_id, Some(challenger_config)).await?;

        // Clear the experiment link on the collection before closing the experiment (finding #10).
        self.collections.set_experiment(collection_id, None).await?;

        let mut map = self.experiments.write().await;
        if let Some(exp) = map.get_mut(&key) {
            exp.status = ExperimentStatus::Closed;
        }
        Ok(())
    }

    pub async fn abandon(&self, collection_id: &str, exp_id: &ExperimentId) -> Result<()> {
        let key = format!("{}:{}", collection_id, exp_id.0);

        // Clear the experiment link on the collection first.
        self.collections.set_experiment(collection_id, None).await?;

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

        Ok(exp.status.clone())
    }

    pub async fn active_experiments(&self) -> Vec<(String, ShadowExperiment)> {
        self.experiments.read().await
            .iter()
            .filter(|(_, exp)| exp.status == ExperimentStatus::Active)
            .map(|(k, exp)| {
                // Key format is "{col_id}:{exp_id}" where exp_id is a UUID (no colons).
                // Split at the last ':' so collection IDs containing colons are preserved.
                let col_id = k.rsplitn(2, ':').nth(1).unwrap_or("").to_string();
                (col_id, exp.clone())
            })
            .collect()
    }
}
