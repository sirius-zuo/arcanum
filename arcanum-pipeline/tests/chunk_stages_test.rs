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
        doc: Some(doc.clone()),
        chunks: vec![],
        graph_chunks: vec![],
        tree_chunks: vec![],
        vectors: vec![],
        tree_vectors: vec![],
        raw_content:   Some(doc.content.clone()),
        canonical_json: None,
        snapshot_document_id: None,
        snapshot_version_num: None,
        snapshot_uri: None,
        canonical_uri: None,
        pending_version: None,
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

#[tokio::test]
async fn shadow_write_ctx_writes_to_shadow_namespace() {
    use arcanum_core::traits::{Embedder, VectorStore, VectorQuery, ScoredChunk, Chunker};
    use arcanum_core::types::*;
    use arcanum_pipeline::stages::{make_vector_chunk_stage, ShadowWriteContext};
    use arcanum_middleware::CircuitBreaker;
    use std::time::Duration;
    use async_trait::async_trait;

    struct RecordingVectorStore(Arc<std::sync::Mutex<Vec<String>>>);
    #[async_trait]
    impl VectorStore for RecordingVectorStore {
        async fn upsert(&self, collection: &str, _chunks: Vec<IndexedChunk>) -> arcanum_core::Result<()> {
            self.0.lock().unwrap().push(collection.to_string());
            Ok(())
        }
        async fn search(&self, _: &str, _: &VectorQuery) -> arcanum_core::Result<Vec<ScoredChunk>> { Ok(vec![]) }
        async fn delete(&self, _: &str, _: &[ChunkId]) -> arcanum_core::Result<()> { Ok(()) }
        async fn collection_exists(&self, _: &str) -> arcanum_core::Result<bool> { Ok(true) }
        async fn delete_by_source_uri(&self, _: &str, _: &str) -> arcanum_core::Result<()> { Ok(()) }
    }

    struct ConstEmbedder;
    #[async_trait]
    impl Embedder for ConstEmbedder {
        async fn embed(&self, texts: Vec<String>) -> arcanum_core::Result<Vec<Vector>> {
            Ok(texts.iter().map(|_| Vector(vec![0.1])).collect())
        }
        fn dimension(&self) -> usize { 1 }
    }

    struct OneChunkChunker;
    #[async_trait]
    impl Chunker for OneChunkChunker {
        async fn chunk(&self, doc: &RawDocument) -> arcanum_core::Result<Vec<Chunk>> {
            Ok(vec![Chunk {
                id: ChunkId::new(),
                text: String::from_utf8_lossy(&doc.content).to_string(),
                document_id: doc.id.clone(),
                collection_id: CollectionId("placeholder".into()),
                position: ChunkPosition { start: 0, end: doc.content.len(), index: 0 },
                metadata: ChunkMetadata::default(),
                provenance: Default::default(),
            }])
        }
    }

    let shadow_namespaces: Arc<std::sync::Mutex<Vec<String>>> = Arc::new(std::sync::Mutex::new(vec![]));
    let recording_store = Arc::new(RecordingVectorStore(shadow_namespaces.clone()));

    let doc = make_raw_doc("hello shadow world");
    let state = make_state(doc);

    let shadow_ctx = ShadowWriteContext {
        chunker:              Arc::new(OneChunkChunker),
        shadow_collection_id: "my-col__shadow_test-exp-id".to_string(),
        embedder:             Arc::new(ConstEmbedder),
        vector_store:         recording_store,
        vector_store_cb:      Arc::new(CircuitBreaker::new("test", 5, Duration::from_secs(30))),
    };

    let stage = make_vector_chunk_stage(
        state.clone(),
        Arc::new(OneChunkChunker),
        Some(shadow_ctx),
    );
    (stage.run)(std::collections::HashMap::new()).await.unwrap();

    // Give the spawned shadow task a moment to complete
    tokio::time::sleep(Duration::from_millis(50)).await;

    let written = shadow_namespaces.lock().unwrap().clone();
    assert!(
        written.contains(&"my-col__shadow_test-exp-id".to_string()),
        "shadow write must upsert to the shadow namespace; got: {:?}", written
    );
}

#[tokio::test]
async fn raptor_build_uses_tree_vectors_not_vector_embeddings() {
    use arcanum_core::traits::{Embedder, TreeStore};
    use arcanum_core::types::*;
    use arcanum_pipeline::stages::{make_tree_embed_stage, make_raptor_build_stage};
    use arcanum_middleware::CircuitBreaker;
    use std::time::Duration;
    use async_trait::async_trait;

    // TreeStore that records what leaf texts (level 0 nodes) it received
    struct RecordingTreeStore(Arc<std::sync::Mutex<Vec<String>>>);
    #[async_trait]
    impl TreeStore for RecordingTreeStore {
        async fn insert_node(&self, _: &str, node: TreeNode) -> arcanum_core::Result<()> {
            if node.level == 0 {
                self.0.lock().unwrap().push(node.text);
            }
            Ok(())
        }
        async fn get_level(&self, _: &str, _: u32) -> arcanum_core::Result<Vec<TreeNode>> { Ok(vec![]) }
        async fn get_children(&self, _: &TreeNodeId) -> arcanum_core::Result<Vec<TreeNode>> { Ok(vec![]) }
        async fn delete_by_source_uri(&self, _: &str, _: &str) -> arcanum_core::Result<()> { Ok(()) }
    }

    struct DistinctEmbedder;
    #[async_trait]
    impl Embedder for DistinctEmbedder {
        async fn embed(&self, texts: Vec<String>) -> arcanum_core::Result<Vec<Vector>> {
            // Return a unique vector per text based on its index in the batch
            Ok(texts.iter().enumerate().map(|(i, _)| Vector(vec![i as f32])).collect())
        }
        fn dimension(&self) -> usize { 1 }
    }

    let tree_chunk_texts = vec!["tree-chunk-A".to_string(), "tree-chunk-B".to_string()];

    let state = Arc::new(Mutex::new(IngestionState {
        source: Source::Raw {
            content: b"hello".to_vec(),
            mime_hint: Some("text/plain".to_string()),
            uri: "test://doc".to_string(),
        },
        collection_id: CollectionId("col".into()),
        doc: Some(RawDocument {
            id: DocumentId::new(),
            content: b"hello".to_vec(),
            mime_type: "text/plain".to_string(),
            source_uri: "test://doc".to_string(),
            metadata: Default::default(),
        }),
        chunks:       vec![],
        graph_chunks: vec![],
        raw_content:   Some(b"hello".to_vec()),
        canonical_json: None,
        snapshot_document_id: None,
        snapshot_version_num: None,
        snapshot_uri: None,
        canonical_uri: None,
        pending_version: None,
        // tree_chunks has 2 entries
        tree_chunks: tree_chunk_texts.iter().map(|t| Chunk {
            id: ChunkId::new(),
            text: t.clone(),
            document_id: DocumentId::new(),
            collection_id: CollectionId("col".into()),
            position: ChunkPosition { start: 0, end: t.len(), index: 0 },
            metadata: ChunkMetadata::default(),
                provenance: Default::default(),
        }).collect(),
        vectors:       vec![Vector(vec![99.0]), Vector(vec![99.0])],  // 2 wrong vector embeddings
        tree_vectors:  vec![],  // will be filled by tree_embed_stage
    }));

    let cb = Arc::new(CircuitBreaker::new("test", 5, Duration::from_secs(30)));
    let received_texts: Arc<std::sync::Mutex<Vec<String>>> = Arc::new(std::sync::Mutex::new(vec![]));
    let tree_store = Arc::new(RecordingTreeStore(received_texts.clone()));

    // Run tree_embed_stage first
    let embed_stage = make_tree_embed_stage(state.clone(), Arc::new(DistinctEmbedder), cb.clone());
    (embed_stage.run)(std::collections::HashMap::new()).await.unwrap();

    // Then run raptor_build_stage
    let raptor_stage = make_raptor_build_stage(state.clone(), tree_store, 1);
    (raptor_stage.run)(std::collections::HashMap::new()).await.unwrap();

    let got = received_texts.lock().unwrap().clone();
    assert_eq!(got, tree_chunk_texts,
        "raptor must receive tree chunk texts (not vector chunks); got: {:?}", got);
}
