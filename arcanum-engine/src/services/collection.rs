use arcanum_core::{config::ArcanumConfig, types::*, Result, ArcanumError};
use std::{collections::HashMap, sync::Arc};
use tokio::sync::RwLock;
use tracing::instrument;
use crate::audit::{AuditLogger, AuditEntry};
use crate::auth::{AuthMiddleware, ApiKeyClaims};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CollectionInfo {
    pub id: CollectionId,
    pub description: String,
    pub chunk_count: usize,
    pub chunker_config: Option<PerBackendChunkConfig>,
    pub experiment: Option<ExperimentId>,  // active experiment ID if any
}

#[derive(Debug)]
pub struct CollectionService {
    collections: Arc<RwLock<HashMap<String, CollectionInfo>>>,
    audit: Arc<AuditLogger>,
    auth: Arc<AuthMiddleware>,
}

impl CollectionService {
    pub fn new(_config: ArcanumConfig, audit: Arc<AuditLogger>, auth: Arc<AuthMiddleware>) -> Self {
        Self { collections: Arc::new(RwLock::new(HashMap::new())), audit, auth }
    }

    #[instrument(skip(self, claims), fields(collection_id = %id.0), err)]
    pub async fn create(&self, id: CollectionId, description: String, claims: &ApiKeyClaims) -> Result<()> {
        // Only admins may create collections.
        if !claims.is_admin {
            return Err(ArcanumError::Auth("only admins may create collections".into()));
        }
        let mut map = self.collections.write().await;
        if map.contains_key(&id.0) {
            return Err(ArcanumError::Storage(format!("collection '{}' already exists", id.0)));
        }
        map.insert(id.0.clone(), CollectionInfo { id: id.clone(), description, chunk_count: 0, chunker_config: None, experiment: None });
        self.audit.log(AuditEntry {
            operation: "create_collection".into(), user_id: claims.user_id.clone(),
            collection_id: id.0, result: "ok".into(),
        }).await;
        Ok(())
    }

    /// Returns only collections the caller is permitted to see.
    #[instrument(skip(self, claims))]
    pub async fn list(&self, claims: &ApiKeyClaims) -> Vec<CollectionInfo> {
        let all = self.collections.read().await;
        all.values()
            .filter(|c| self.auth.can_access_collection(claims, &c.id.0))
            .cloned()
            .collect()
    }

    #[instrument(skip(self, claims), fields(collection_id = id), err)]
    pub async fn delete(&self, id: &str, claims: &ApiKeyClaims) -> Result<()> {
        if !self.auth.can_access_collection(claims, id) {
            return Err(ArcanumError::Auth(format!("not authorised to delete collection '{}'", id)));
        }
        let removed = self.collections.write().await.remove(id).is_some();
        if !removed { return Err(ArcanumError::NotFound(format!("collection '{}'", id))); }
        self.audit.log(AuditEntry {
            operation: "delete_collection".into(), user_id: claims.user_id.clone(),
            collection_id: id.to_string(), result: "ok".into(),
        }).await;
        Ok(())
    }

    /// Returns a collection by ID.
    pub async fn get(&self, id: &str) -> Result<CollectionInfo> {
        self.collections.read().await.get(id)
            .cloned()
            .ok_or_else(|| ArcanumError::NotFound(format!("collection '{}'", id)))
    }

    /// Update the chunker config for a collection (used by promote).
    pub async fn set_chunker_config(
        &self,
        id: &str,
        config: Option<PerBackendChunkConfig>,
    ) -> Result<()> {
        let mut map = self.collections.write().await;
        let col = map.get_mut(id)
            .ok_or_else(|| ArcanumError::NotFound(format!("collection '{}'", id)))?;
        col.chunker_config = config;
        Ok(())
    }

    /// Set or clear the active experiment ID on a collection.
    pub async fn set_experiment(
        &self,
        id: &str,
        exp_id: Option<ExperimentId>,
    ) -> Result<()> {
        let mut map = self.collections.write().await;
        let col = map.get_mut(id)
            .ok_or_else(|| ArcanumError::NotFound(format!("collection '{}'", id)))?;
        col.experiment = exp_id;
        Ok(())
    }
}
