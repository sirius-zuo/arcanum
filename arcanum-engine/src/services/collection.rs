use arcanum_core::{config::ArcanumConfig, types::*, Result, ArcanumError};
use std::{collections::HashMap, sync::Arc};
use tokio::sync::RwLock;
use crate::audit::{AuditLogger, AuditEntry};
use crate::auth::{AuthMiddleware, ApiKeyClaims};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CollectionInfo {
    pub id: CollectionId,
    pub description: String,
    pub chunk_count: usize,
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

    pub async fn create(&self, id: CollectionId, description: String, claims: &ApiKeyClaims) -> Result<()> {
        // Only admins may create collections.
        if !claims.is_admin {
            return Err(ArcanumError::Auth("only admins may create collections".into()));
        }
        let mut map = self.collections.write().await;
        if map.contains_key(&id.0) {
            return Err(ArcanumError::Storage(format!("collection '{}' already exists", id.0)));
        }
        map.insert(id.0.clone(), CollectionInfo { id: id.clone(), description, chunk_count: 0 });
        self.audit.log(AuditEntry {
            operation: "create_collection".into(), user_id: claims.user_id.clone(),
            collection_id: id.0, result: "ok".into(),
        }).await;
        Ok(())
    }

    /// Returns only collections the caller is permitted to see.
    pub async fn list(&self, claims: &ApiKeyClaims) -> Vec<CollectionInfo> {
        let all = self.collections.read().await;
        all.values()
            .filter(|c| self.auth.can_access_collection(claims, &c.id.0))
            .cloned()
            .collect()
    }

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
}
