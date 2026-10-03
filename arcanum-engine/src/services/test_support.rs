//! Shared fixtures for service tests.

use arcanum_core::traits::{ChunkMetadataStore, InMemoryChunkMetadataStore, Retriever};
use arcanum_core::types::*;
use arcanum_core::ArcanumError;
use std::sync::Arc;

/// Returns one Source chunk whose id is seeded in the registry.
pub(crate) struct RegistryRetriever {
    pub(crate) chunk: Chunk,
    pub(crate) strategy: RetrievalStrategy,
}

#[async_trait::async_trait]
impl Retriever for RegistryRetriever {
    async fn retrieve(&self, _q: &Query) -> arcanum_core::Result<Vec<RetrievedChunk>> {
        Ok(vec![RetrievedChunk {
            indexed_chunk: IndexedChunk {
                chunk: self.chunk.clone(),
                vector: Vector(vec![]),
                token_vectors: None,
                store_id: "s1".into(),
            },
            score: 0.9,
            strategy: self.strategy.clone(),
            kind: ChunkKind::Source,
        }])
    }
    fn strategy(&self) -> RetrievalStrategy {
        self.strategy.clone()
    }
}

pub(crate) struct FailingRetriever(pub(crate) RetrievalStrategy);

#[async_trait::async_trait]
impl Retriever for FailingRetriever {
    async fn retrieve(&self, _q: &Query) -> arcanum_core::Result<Vec<RetrievedChunk>> {
        Err(ArcanumError::Retrieval("boom".into()))
    }
    fn strategy(&self) -> RetrievalStrategy {
        self.0.clone()
    }
}

pub(crate) const DOC_TEXT: &str = "The quick brown fox jumps over the lazy dog.";

pub(crate) async fn seeded_registry() -> (Arc<InMemoryChunkMetadataStore>, Chunk) {
    let store = Arc::new(InMemoryChunkMetadataStore::new());
    let rec = ChunkMetadataRecord {
        chunk_id: ChunkId::new(),
        document_id: DocumentId::new(),
        collection_id: "col1".into(),
        version_num: 1,
        backend: ChunkBackend::Vector,
        text: DOC_TEXT.into(),
        chunk_index: 0,
        source_uri: "raw://doc".into(),
        snapshot_uri: "file:///snap/doc/1.raw".into(),
        canonical_uri: None,
        page: None,
        section: None,
        block_ids: vec![],
        offset_start: 0,
        offset_end: DOC_TEXT.len(),
        ingested_at: chrono::Utc::now(),
    };
    store.put(&rec).await.unwrap();
    let chunk = rec.to_chunk();
    (store, chunk)
}
