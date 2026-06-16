use async_trait::async_trait;
use crate::{
    types::{DocumentId, DocumentVersion, VersioningPolicy},
    Result,
};

#[async_trait]
pub trait DocumentVersionStore: Send + Sync {
    async fn get_latest(
        &self,
        source_uri:    &str,
        collection_id: &str,
    ) -> Result<Option<DocumentVersion>>;

    async fn add_version(&self, version: DocumentVersion) -> Result<()>;

    async fn supersede_active(&self, document_id: &DocumentId) -> Result<()>;

    async fn list_versions(
        &self,
        document_id: &DocumentId,
    ) -> Result<Vec<DocumentVersion>>;

    async fn get_versioning_policy(&self, collection_id: &str) -> Result<VersioningPolicy>;

    async fn set_versioning_policy(
        &self,
        collection_id: &str,
        policy:        VersioningPolicy,
    ) -> Result<()>;

    /// Remove all version records for a given source URI in a collection.
    /// Used when a document is deleted from the system.
    async fn delete_by_source_uri(&self, collection_id: &str, source_uri: &str) -> Result<()>;
}

/// No-op implementation for tests and dev setups without Postgres.
/// Every document is treated as new; no version history is kept.
pub struct NoOpDocumentVersionStore;

#[async_trait]
impl DocumentVersionStore for NoOpDocumentVersionStore {
    async fn get_latest(&self, _: &str, _: &str) -> Result<Option<DocumentVersion>> {
        Ok(None)
    }
    async fn add_version(&self, _: DocumentVersion) -> Result<()> { Ok(()) }
    async fn supersede_active(&self, _: &DocumentId) -> Result<()> { Ok(()) }
    async fn list_versions(&self, _: &DocumentId) -> Result<Vec<DocumentVersion>> { Ok(vec![]) }
    async fn get_versioning_policy(&self, _: &str) -> Result<VersioningPolicy> {
        Ok(VersioningPolicy::Replace)
    }
    async fn set_versioning_policy(&self, _: &str, _: VersioningPolicy) -> Result<()> { Ok(()) }
    async fn delete_by_source_uri(&self, _: &str, _: &str) -> Result<()> { Ok(()) }
}
