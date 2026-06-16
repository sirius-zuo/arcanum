use async_trait::async_trait;
use crate::{types::{DocumentId, SnapshotLocation}, Result};

#[async_trait]
pub trait SnapshotStore: Send + Sync {
    async fn store(
        &self,
        document_id: &DocumentId,
        version:     u32,
        raw:         &[u8],
        canonical:   Option<&serde_json::Value>,
    ) -> Result<SnapshotLocation>;

    async fn fetch_raw(&self, uri: &str) -> Result<Vec<u8>>;
    async fn fetch_canonical(&self, uri: &str) -> Result<Option<serde_json::Value>>;
}
