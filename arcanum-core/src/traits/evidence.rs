use crate::{
    types::{ChunkId, ChunkMetadataRecord, DocumentId, EntityId, GcReport, ProofChain, TreeNodeId},
    Result,
};
use async_trait::async_trait;

#[async_trait]
pub trait EvidenceResolver: Send + Sync {
    async fn resolve_chunk(&self, chunk_id: &ChunkId) -> Result<ProofChain>;
    async fn resolve_tree_node(&self, node_id: &TreeNodeId) -> Result<ProofChain>;
    async fn resolve_entity(&self, entity_id: &EntityId) -> Result<ProofChain>;
    async fn resolve_relation(
        &self,
        source_id: &EntityId,
        relation_type: &str,
        target_id: &EntityId,
    ) -> Result<ProofChain>;
}

#[async_trait]
pub trait ChunkMetadataStore: Send + Sync {
    async fn put(&self, record: &ChunkMetadataRecord) -> Result<()>;
    async fn get(&self, chunk_id: &ChunkId) -> Result<Option<ChunkMetadataRecord>>;
    /// Fetch several records at once. Missing ids are absent from the result and the
    /// order is unspecified.
    async fn get_many(&self, ids: &[ChunkId]) -> Result<Vec<ChunkMetadataRecord>>;
    async fn delete_by_source_uri(&self, collection_id: &str, source_uri: &str) -> Result<()>;
    /// Delete all chunk_metadata rows belonging to a specific document version, returning
    /// the chunk IDs that were removed. Unlike `delete_by_source_uri`, this is scoped to a
    /// single version, so it is safe to call even when other versions of the same document
    /// (sharing the same source_uri) are still active or retained.
    async fn delete_by_document_version(
        &self,
        document_id: &DocumentId,
        version_num: u32,
    ) -> Result<Vec<ChunkId>>;
}

#[async_trait]
pub trait GcWorker: Send + Sync {
    async fn run_once(&self) -> Result<GcReport>;
}
