use arcanum_core::{Result, ArcanumError, types::CollectionId};
use std::collections::HashMap;
use tokio::sync::RwLock;
use tracing::instrument;

#[derive(Debug, Clone)]
pub struct CollectionMeta {
    pub id: CollectionId,
    pub vector_dimension: usize,
}

pub struct CollectionManager {
    collections: RwLock<HashMap<String, CollectionMeta>>,
}

impl CollectionManager {
    pub fn new() -> Self {
        Self { collections: RwLock::new(HashMap::new()) }
    }

    #[instrument(skip(self), fields(collection_id = ?id, vector_dim), err)]
    pub async fn create(&self, id: CollectionId, vector_dim: usize) -> Result<()> {
        let mut map = self.collections.write().await;
        if map.contains_key(&id.0) {
            return Err(ArcanumError::Storage(format!("collection '{}' already exists", id.0)));
        }
        map.insert(id.0.clone(), CollectionMeta { id, vector_dimension: vector_dim });
        Ok(())
    }

    #[instrument(skip(self), fields(collection_id = id))]
    pub async fn get(&self, id: &str) -> Result<CollectionMeta> {
        self.collections.read().await.get(id)
            .cloned()
            .ok_or_else(|| ArcanumError::NotFound(format!("collection '{}'", id)))
    }

    #[instrument(skip(self), fields(collection_id = id))]
    pub async fn delete(&self, id: &str) -> Result<()> {
        self.collections.write().await.remove(id);
        Ok(())
    }

    #[instrument(skip(self))]
    pub async fn list(&self) -> Vec<CollectionMeta> {
        self.collections.read().await.values().cloned().collect()
    }
}
