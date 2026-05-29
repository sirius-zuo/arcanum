use arcanum_core::{config::ArcanumConfig, types::*, Result, ArcanumError};
use std::{collections::HashMap, sync::Arc};
use tokio::sync::RwLock;
use crate::audit::{AuditLogger, AuditEntry};

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
}

impl CollectionService {
    pub fn new(_config: ArcanumConfig, audit: Arc<AuditLogger>) -> Self {
        Self { collections: Arc::new(RwLock::new(HashMap::new())), audit }
    }

    pub async fn create(&self, id: CollectionId, description: String, user_id: &str) -> Result<()> {
        let mut map = self.collections.write().await;
        if map.contains_key(&id.0) {
            return Err(ArcanumError::Storage(format!("collection '{}' already exists", id.0)));
        }
        map.insert(id.0.clone(), CollectionInfo { id: id.clone(), description, chunk_count: 0 });
        self.audit.log(AuditEntry {
            operation: "create_collection".into(), user_id: user_id.to_string(),
            collection_id: id.0, result: "ok".into(),
        }).await;
        Ok(())
    }

    pub async fn list(&self) -> Vec<CollectionInfo> {
        self.collections.read().await.values().cloned().collect()
    }

    pub async fn delete(&self, id: &str, user_id: &str) -> Result<()> {
        let removed = self.collections.write().await.remove(id).is_some();
        if !removed { return Err(ArcanumError::NotFound(format!("collection '{}'", id))); }
        self.audit.log(AuditEntry {
            operation: "delete_collection".into(), user_id: user_id.to_string(),
            collection_id: id.to_string(), result: "ok".into(),
        }).await;
        Ok(())
    }
}
