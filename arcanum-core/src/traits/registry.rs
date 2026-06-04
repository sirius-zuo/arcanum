use async_trait::async_trait;
use crate::Result;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegistryStatus {
    Clean,
    Replacing,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RegistryEntry {
    /// None while status is Replacing (cleanup in progress).
    pub content_hash: Option<String>,
    pub status: RegistryStatus,
}

#[async_trait]
pub trait DocumentRegistry: Send + Sync {
    /// Returns the current entry, or None if this document has never been registered.
    async fn get_entry(&self, source_uri: &str, collection_id: &str) -> Result<Option<RegistryEntry>>;

    /// Mark a document as being replaced. Written before any store deletion begins.
    /// Idempotent — safe to call multiple times.
    async fn set_replacing(&self, source_uri: &str, collection_id: &str) -> Result<()>;

    /// Record a successfully ingested document. Called by the worker on pipeline success.
    async fn register(&self, source_uri: &str, collection_id: &str, content_hash: &str) -> Result<()>;

    /// Remove a registry entry. No-op if the entry doesn't exist.
    async fn deregister(&self, source_uri: &str, collection_id: &str) -> Result<()>;

    /// Atomically claim replacing status only if the entry is currently Clean (or absent).
    /// Returns Ok(true) if this caller successfully claimed it, Ok(false) if another worker
    /// is already replacing this document (caller should skip cleanup).
    /// Default implementation calls set_replacing and always returns Ok(true) — implementors
    /// that want CAS semantics should override this.
    async fn try_set_replacing(&self, source_uri: &str, collection_id: &str) -> Result<bool> {
        self.set_replacing(source_uri, collection_id).await?;
        Ok(true)
    }
}

/// Dedup disabled: every ingest always processes. Used when no registry is configured.
pub struct NoOpDocumentRegistry;

#[async_trait]
impl DocumentRegistry for NoOpDocumentRegistry {
    async fn get_entry(&self, _: &str, _: &str) -> Result<Option<RegistryEntry>> {
        Ok(None)
    }
    async fn set_replacing(&self, _: &str, _: &str) -> Result<()> { Ok(()) }
    async fn register(&self, _: &str, _: &str, _: &str) -> Result<()> { Ok(()) }
    async fn deregister(&self, _: &str, _: &str) -> Result<()> { Ok(()) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn no_op_registry_always_returns_none() {
        let r = NoOpDocumentRegistry;
        assert!(r.get_entry("test://uri", "col1").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn no_op_registry_all_mutations_succeed() {
        let r = NoOpDocumentRegistry;
        r.set_replacing("uri", "col").await.unwrap();
        r.register("uri", "col", "abc123").await.unwrap();
        r.deregister("uri", "col").await.unwrap();
    }
}
