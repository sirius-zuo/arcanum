use arcanum_core::traits::{Chunker, GraphStore, Source, TextEnricher};
use arcanum_core::types::*;
use arcanum_core::types::{EnrichRequest, EnrichedText};
use arcanum_ingestion::{FixedSizeChunker, SemanticChunker};
use arcanum_pipeline::{
    ingestion_state::IngestionState,
    stages::{make_vector_chunk_stage, make_graph_chunk_stage, make_tree_chunk_stage},
};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::sync::Mutex;

fn make_raw_doc(text: &str) -> RawDocument {
    RawDocument {
        id: DocumentId::new(),
        content: text.as_bytes().to_vec(),
        mime_type: "text/plain".to_string(),
        source_uri: "test://doc.txt".to_string(),
        metadata: Default::default(),
    }
}

fn make_state(doc: RawDocument) -> Arc<Mutex<IngestionState>> {
    Arc::new(Mutex::new(IngestionState {
        source: Source::Raw {
            content: doc.content.clone(),
            mime_hint: Some("text/plain".to_string()),
            uri: doc.source_uri.clone(),
        },
        collection_id: CollectionId("test-collection".into()),
        doc: Some(doc),
        chunks: vec![],
        graph_chunks: vec![],
        tree_chunks: vec![],
        vectors: vec![],
    }))
}

#[tokio::test]
async fn vector_chunk_stage_writes_to_chunks_field() {
    let doc = make_raw_doc(
        "Hello world. This is a test document with enough text to chunk.",
    );
    let state = make_state(doc);
    let chunker = Arc::new(FixedSizeChunker::new(20, 5)) as Arc<dyn Chunker>;
    let stage = make_vector_chunk_stage(state.clone(), chunker, None);
    let ctx = std::collections::HashMap::new();
    (stage.run)(ctx).await.unwrap();
    let s = state.lock().await;
    assert!(
        !s.chunks.is_empty(),
        "vector chunk stage should populate state.chunks"
    );
    assert!(
        s.graph_chunks.is_empty(),
        "vector chunk stage must not touch graph_chunks"
    );
    assert!(
        s.tree_chunks.is_empty(),
        "vector chunk stage must not touch tree_chunks"
    );
}

#[tokio::test]
async fn graph_chunk_stage_writes_to_graph_chunks_field() {
    let doc = make_raw_doc(
        "Hello world. This is a test document with enough text to chunk.",
    );
    let state = make_state(doc);
    let chunker = Arc::new(SemanticChunker::new(50)) as Arc<dyn Chunker>;
    let stage = make_graph_chunk_stage(state.clone(), chunker);
    let ctx = std::collections::HashMap::new();
    (stage.run)(ctx).await.unwrap();
    let s = state.lock().await;
    assert!(
        !s.graph_chunks.is_empty(),
        "graph chunk stage should populate state.graph_chunks"
    );
    assert!(s.chunks.is_empty(), "graph chunk stage must not touch state.chunks");
    assert!(
        s.tree_chunks.is_empty(),
        "graph chunk stage must not touch tree_chunks"
    );
}

#[tokio::test]
async fn tree_chunk_stage_writes_to_tree_chunks_field() {
    let doc = make_raw_doc(
        "Hello world. This is a test document with enough text to chunk.",
    );
    let state = make_state(doc);
    let chunker = Arc::new(FixedSizeChunker::new(30, 5)) as Arc<dyn Chunker>;
    let stage = make_tree_chunk_stage(state.clone(), chunker);
    let ctx = std::collections::HashMap::new();
    (stage.run)(ctx).await.unwrap();
    let s = state.lock().await;
    assert!(
        !s.tree_chunks.is_empty(),
        "tree chunk stage should populate state.tree_chunks"
    );
    assert!(s.chunks.is_empty(), "tree chunk stage must not touch state.chunks");
    assert!(
        s.graph_chunks.is_empty(),
        "tree chunk stage must not touch graph_chunks"
    );
}

#[tokio::test]
async fn vector_and_graph_stages_produce_different_chunk_counts_with_different_chunkers() {
    let text = "Hello world. This is sentence one. This is sentence two. More text here.";
    let doc = make_raw_doc(text);
    let doc2 = make_raw_doc(text);
    let state = make_state(doc);
    let state2 = make_state(doc2);

    // Fixed chunker: many small chunks
    let vector_chunker = Arc::new(FixedSizeChunker::new(15, 2)) as Arc<dyn Chunker>;
    // Semantic chunker: fewer, sentence-boundary chunks
    let graph_chunker = Arc::new(SemanticChunker::new(200)) as Arc<dyn Chunker>;

    let v_stage = make_vector_chunk_stage(state.clone(), vector_chunker, None);
    let g_stage = make_graph_chunk_stage(state2.clone(), graph_chunker);

    (v_stage.run)(std::collections::HashMap::new()).await.unwrap();
    (g_stage.run)(std::collections::HashMap::new()).await.unwrap();

    let vector_count = state.lock().await.chunks.len();
    let graph_count = state2.lock().await.graph_chunks.len();
    assert_ne!(
        vector_count, graph_count,
        "different chunkers should produce different chunk counts: vector={}, graph={}",
        vector_count, graph_count
    );
}

#[tokio::test]
async fn entity_extract_is_noop_when_graph_chunks_empty() {
    use arcanum_core::traits::TextEnricher;
    use arcanum_core::types::*;
    use arcanum_pipeline::stages::make_entity_extract_stage;
    use async_trait::async_trait;

    struct CountingGraphStore(Arc<AtomicUsize>);
    #[async_trait]
    impl GraphStore for CountingGraphStore {
        async fn upsert_entities(&self, _: &str, _: Vec<Entity>) -> arcanum_core::Result<()> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
        async fn upsert_relations(&self, _: &str, _: Vec<Relation>) -> arcanum_core::Result<()> { Ok(()) }
        async fn query(&self, _: &str, _: &arcanum_core::traits::GraphQuery) -> arcanum_core::Result<Vec<Entity>> { Ok(vec![]) }
        async fn get_relations(&self, _: &EntityId) -> arcanum_core::Result<Vec<Relation>> { Ok(vec![]) }
        async fn delete_by_source_uri(&self, _: &str, _: &str) -> arcanum_core::Result<()> { Ok(()) }
    }

    struct NoopEnricher;
    #[async_trait]
    impl TextEnricher for NoopEnricher {
        async fn enrich(&self, req: EnrichRequest) -> arcanum_core::Result<EnrichedText> {
            Ok(EnrichedText(req.text))
        }
    }

    let doc = make_raw_doc("some text");
    let state = make_state(doc);
    // graph_chunks is empty — entity extract must skip, not fall back to vector chunks
    assert!(state.lock().await.graph_chunks.is_empty());

    let upsert_count = Arc::new(AtomicUsize::new(0));
    let graph_store = Arc::new(CountingGraphStore(upsert_count.clone()));
    let stage = make_entity_extract_stage(
        state.clone(),
        Arc::new(NoopEnricher),
        graph_store,
    );
    (stage.run)(std::collections::HashMap::new()).await.unwrap();

    assert_eq!(upsert_count.load(Ordering::SeqCst), 0,
        "entity_extract must not call upsert_entities when graph_chunks is empty");
}
