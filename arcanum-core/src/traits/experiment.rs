use async_trait::async_trait;
use crate::{
    types::{ExperimentId, PerBackendChunkConfig},
    ArcanumError, Result,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tokio::sync::RwLock;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExperimentStatus {
    Active,
    ReadyToPromote,
    Closed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExperimentMetrics {
    pub champion_recall_at_5:   f32,
    pub challenger_recall_at_5: f32,
    pub sample_size:            usize,
    pub computed_at:            String,  // ISO-8601
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

#[async_trait]
pub trait ExperimentStore: Send + Sync {
    /// Atomically insert `exp` iff `collection_id` has no Active experiment.
    /// Err(ArcanumError::Storage("...already has an active experiment...")) otherwise.
    async fn try_start(&self, collection_id: &str, exp: &ShadowExperiment) -> Result<()>;
    async fn get(&self, collection_id: &str, exp_id: &ExperimentId) -> Result<Option<ShadowExperiment>>;
    /// Full-row update by (collection_id, exp.id). Err(NotFound) if absent or if the
    /// stored row is already Closed (guards stale writes from resurrecting a closed
    /// experiment).
    async fn update(&self, collection_id: &str, exp: &ShadowExperiment) -> Result<()>;
    /// All Active experiments across collections, as (collection_id, experiment).
    async fn active_experiments(&self) -> Result<Vec<(String, ShadowExperiment)>>;
}

pub struct InMemoryExperimentStore {
    experiments: RwLock<HashMap<String, ShadowExperiment>>, // key: "{col_id}:{exp_id}"
}

impl InMemoryExperimentStore {
    pub fn new() -> Self { Self { experiments: RwLock::new(HashMap::new()) } }
    fn key(collection_id: &str, exp_id: &ExperimentId) -> String {
        format!("{}:{}", collection_id, exp_id.0)
    }
}

impl Default for InMemoryExperimentStore {
    fn default() -> Self { Self::new() }
}

#[async_trait]
impl ExperimentStore for InMemoryExperimentStore {
    async fn try_start(&self, collection_id: &str, exp: &ShadowExperiment) -> Result<()> {
        // Single write lock = atomic check-then-insert (preserves TOCTOU fix, finding #7).
        // Assumes collection ids contain no ':' (key format "{col}:{uuid}"); the Postgres
        // impl matches collection_id by column, exact-match, and is unaffected.
        let mut map = self.experiments.write().await;
        let has_active = map.iter().any(|(k, e)| {
            k.starts_with(&format!("{}:", collection_id)) && e.status == ExperimentStatus::Active
        });
        if has_active {
            return Err(ArcanumError::Storage(format!(
                "collection '{}' already has an active experiment", collection_id)));
        }
        map.insert(Self::key(collection_id, &exp.id), exp.clone());
        Ok(())
    }

    async fn get(&self, collection_id: &str, exp_id: &ExperimentId) -> Result<Option<ShadowExperiment>> {
        Ok(self.experiments.read().await.get(&Self::key(collection_id, exp_id)).cloned())
    }

    async fn update(&self, collection_id: &str, exp: &ShadowExperiment) -> Result<()> {
        let mut map = self.experiments.write().await;
        let key = Self::key(collection_id, &exp.id);
        match map.get_mut(&key) {
            Some(slot) if slot.status == ExperimentStatus::Closed => Err(ArcanumError::NotFound(format!(
                "experiment '{}' (missing or already closed)", exp.id.0))),
            Some(slot) => { *slot = exp.clone(); Ok(()) }
            None => Err(ArcanumError::NotFound(format!(
                "experiment '{}' (missing or already closed)", exp.id.0))),
        }
    }

    async fn active_experiments(&self) -> Result<Vec<(String, ShadowExperiment)>> {
        Ok(self.experiments.read().await.iter()
            .filter(|(_, e)| e.status == ExperimentStatus::Active)
            .map(|(k, e)| (k.rsplitn(2, ':').nth(1).unwrap_or("").to_string(), e.clone()))
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_exp() -> ShadowExperiment {
        ShadowExperiment {
            id: ExperimentId::new(),
            challenger_config: PerBackendChunkConfig::default(),
            started_at: chrono::Utc::now().to_rfc3339(),
            status: ExperimentStatus::Active,
            metrics: None,
        }
    }

    #[tokio::test]
    async fn try_start_rejects_second_active_for_same_collection() {
        let store = InMemoryExperimentStore::new();
        store.try_start("col1", &sample_exp()).await.unwrap();
        assert!(store.try_start("col1", &sample_exp()).await.is_err());
        store.try_start("col2", &sample_exp()).await.unwrap(); // other collections unaffected
    }

    #[tokio::test]
    async fn closed_experiment_frees_the_active_slot() {
        let store = InMemoryExperimentStore::new();
        let mut exp = sample_exp();
        store.try_start("col1", &exp).await.unwrap();
        exp.status = ExperimentStatus::Closed;
        store.update("col1", &exp).await.unwrap();
        assert!(store.try_start("col1", &sample_exp()).await.is_ok());
    }

    #[tokio::test]
    async fn update_rejects_a_stale_write_over_an_already_closed_experiment() {
        // Race guard: a get-then-update on a closed experiment must not resurrect it
        // (e.g. a stale update_metrics landing after a concurrent promote/abandon
        // already closed it). Mirrors PostgresExperimentStore's
        // update_rejects_a_stale_write_over_an_already_closed_experiment.
        let store = InMemoryExperimentStore::new();
        let mut exp = sample_exp();
        store.try_start("col1", &exp).await.unwrap();

        let mut closed = exp.clone();
        closed.status = ExperimentStatus::Closed;
        store.update("col1", &closed).await.unwrap();

        // Stale update racing in after the close — must be rejected, not resurrect the row.
        exp.status = ExperimentStatus::Active;
        exp.metrics = Some(ExperimentMetrics {
            champion_recall_at_5: 0.5,
            challenger_recall_at_5: 0.6,
            sample_size: 100,
            computed_at: chrono::Utc::now().to_rfc3339(),
        });
        let err = store.update("col1", &exp).await.unwrap_err();
        assert!(matches!(err, ArcanumError::NotFound(_)));

        // The row must still read back as closed.
        let fetched = store.get("col1", &exp.id).await.unwrap().unwrap();
        assert_eq!(fetched.status, ExperimentStatus::Closed);
    }

    #[tokio::test]
    async fn get_update_roundtrip_and_active_listing() {
        let store = InMemoryExperimentStore::new();
        let exp = sample_exp();
        store.try_start("col1", &exp).await.unwrap();
        assert_eq!(store.get("col1", &exp.id).await.unwrap().unwrap().id, exp.id);
        assert!(store.get("col1", &ExperimentId::new()).await.unwrap().is_none());
        let active = store.active_experiments().await.unwrap();
        assert_eq!(active, vec![("col1".to_string(), exp)]); // needs PartialEq on ShadowExperiment; derive it
    }
}
