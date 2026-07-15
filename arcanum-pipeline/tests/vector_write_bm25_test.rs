use arcanum_core::traits::{Source, VectorStore, VectorQuery, ScoredChunk};
use arcanum_core::types::*;
use arcanum_pipeline::{ingestion_state::IngestionState, stages::make_vector_write_stage};
use arcanum_vector::Bm25Index;
use async_trait::async_trait;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

struct NoopVectorStore;
#[async_trait]
impl VectorStore for NoopVectorStore {
    async fn upsert(&self, _: &str, _: Vec<IndexedChunk>) -> arcanum_core::Result<()> { Ok(()) }
    async fn search(&self, _: &str, _: &VectorQuery) -> arcanum_core::Result<Vec<ScoredChunk>> { Ok(vec![]) }
    async fn delete(&self, _: &str, _: &[ChunkId]) -> arcanum_core::Result<()> { Ok(()) }
    async fn collection_exists(&self, _: &str) -> arcanum_core::Result<bool> { Ok(true) }
    async fn delete_by_source_uri(&self, _: &str, _: &str) -> arcanum_core::Result<()> { Ok(()) }
}

fn make_state_with_chunks() -> Arc<Mutex<IngestionState>> {
    let doc = RawDocument {
        id: DocumentId::new(),
        content: b"hello".to_vec(),
        mime_type: "text/plain".into(),
        source_uri: "test://doc.txt".into(),
        metadata: Default::default(),
    };
    let chunk = Chunk {
        id: ChunkId::new(),
        text: "the quick brown fox jumps".into(),
        document_id: DocumentId::new(),
        collection_id: CollectionId("test-collection".into()),
        position: ChunkPosition { start: 0, end: 25, index: 0 },
        metadata: ChunkMetadata::default(),
        provenance: ChunkProvenance::default(),
    };
    Arc::new(Mutex::new(IngestionState {
        source: Source::Raw { content: doc.content.clone(), mime_hint: Some("text/plain".into()), uri: doc.source_uri.clone() },
        collection_id: CollectionId("test-collection".into()),
        doc: Some(doc.clone()),
        chunks: vec![chunk],
        graph_chunks: vec![],
        tree_chunks: vec![],
        vectors: vec![Vector(vec![0.1, 0.2, 0.3])],
        tree_vectors: vec![],
        raw_content: Some(doc.content.clone()),
        canonical_json: None,
        snapshot_document_id: None,
        snapshot_version_num: None,
        snapshot_uri: None,
        canonical_uri: None,
        pending_version: None,
    }))
}

#[tokio::test]
async fn vector_write_stage_also_populates_bm25_index() {
    let state = make_state_with_chunks();
    let dir = tempfile::tempdir().unwrap();
    let bm25 = Arc::new(Bm25Index::new(dir.path().to_str().unwrap()).unwrap());
    let vector_store: Arc<dyn VectorStore> = Arc::new(NoopVectorStore);
    let cb = Arc::new(arcanum_middleware::CircuitBreaker::new("vector_store", 5, Duration::from_secs(30)));

    let stage = make_vector_write_stage(state, vector_store, cb, None, Some(bm25.clone()));
    (stage.run)(std::collections::HashMap::new()).await.unwrap();

    let results = bm25.search("quick fox", 5).unwrap();
    assert!(!results.is_empty(), "vector_write should have populated the bm25 index");
}
