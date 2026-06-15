// Compatibility shim: DocumentRegistry trait stub for engine.rs (Task 15 migration).
// The engine uses Arc<dyn DocumentRegistry>; this provides a no-op implementation
// until the engine is fully migrated to DocumentVersionStore + SnapshotStore.
use async_trait::async_trait;
use crate::{types::DocumentId, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegistryStatus {
    Clean,
    Replacing,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RegistryEntry {
    pub content_hash: Option<String>,
    pub status: RegistryStatus,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct DocumentEntry {
    pub source_uri: String,
    pub registered_at: i64,
}

#[async_trait]
pub trait DocumentRegistry: Send + Sync {
    async fn get_entry(&self, source_uri: &str, collection_id: &str) -> Result<Option<RegistryEntry>>;
    async fn set_replacing(&self, source_uri: &str, collection_id: &str) -> Result<()>;
    async fn register(&self, source_uri: &str, collection_id: &str, content_hash: &str) -> Result<()>;
    async fn deregister(&self, source_uri: &str, collection_id: &str) -> Result<()>;
    async fn list_by_collection(&self, _collection_id: &str) -> Result<Vec<DocumentEntry>> {
        Ok(vec![])
    }
    async fn try_set_replacing(&self, source_uri: &str, collection_id: &str) -> Result<bool> {
        self.set_replacing(source_uri, collection_id).await?;
        Ok(true)
    }
}

/// No-op implementation — every document is treated as new.
pub struct NoOpDocumentRegistry;

#[async_trait]
impl DocumentRegistry for NoOpDocumentRegistry {
    async fn get_entry(&self, _: &str, _: &str) -> Result<Option<RegistryEntry>> { Ok(None) }
    async fn set_replacing(&self, _: &str, _: &str) -> Result<()> { Ok(()) }
    async fn register(&self, _: &str, _: &str, _: &str) -> Result<()> { Ok(()) }
    async fn deregister(&self, _: &str, _: &str) -> Result<()> { Ok(()) }
    async fn list_by_collection(&self, _: &str) -> Result<Vec<DocumentEntry>> { Ok(vec![]) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn no_op_registry_always_returns_none() {
        let r = NoOpDocumentRegistry;
        assert!(r.get_entry("test://uri", "col1").await.unwrap().is_none());
    }
}
