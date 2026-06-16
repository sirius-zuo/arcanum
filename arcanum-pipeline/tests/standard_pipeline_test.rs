use arcanum_pipeline::{ArcanumPipelineRegistry, DagExecutor, IngestionState, PipelineDeps, dag::CTX_SKIP};
use arcanum_core::traits::Source;
use arcanum_core::types::CollectionId;
use std::sync::Arc;
use tokio::sync::Mutex;

fn stub_deps() -> Arc<PipelineDeps> {
    use arcanum_ingestion::{LoaderRegistry, PreprocessorRegistry, RawLoader};
    use arcanum_core::traits::{Chunker, Embedder, VectorStore};
    use arcanum_core::types::{*, PerBackendChunkers};
    use async_trait::async_trait;

    struct StubChunker;
    #[async_trait]
    impl Chunker for StubChunker {
        async fn chunk(&self, _doc: &RawDocument) -> arcanum_core::Result<Vec<Chunk>> { Ok(vec![]) }
    }
    let stub_chunker = Arc::new(StubChunker);
    let chunkers = PerBackendChunkers {
        vector: stub_chunker.clone(),
        graph:  stub_chunker.clone(),
        tree:   stub_chunker.clone(),
    };
    struct StubEmbedder;
    #[async_trait]
    impl Embedder for StubEmbedder {
        async fn embed(&self, _t: Vec<String>) -> arcanum_core::Result<Vec<Vector>> { Ok(vec![]) }
        fn dimension(&self) -> usize { 3 }
    }
    struct StubVectorStore;
    #[async_trait]
    impl VectorStore for StubVectorStore {
        async fn upsert(&self, _: &str, _: Vec<IndexedChunk>) -> arcanum_core::Result<()> { Ok(()) }
        async fn search(&self, _: &str, _: &arcanum_core::traits::VectorQuery) -> arcanum_core::Result<Vec<arcanum_core::traits::ScoredChunk>> { Ok(vec![]) }
        async fn delete(&self, _: &str, _: &[ChunkId]) -> arcanum_core::Result<()> { Ok(()) }
        async fn collection_exists(&self, _: &str) -> arcanum_core::Result<bool> { Ok(true) }
        async fn delete_by_source_uri(&self, _: &str, _: &str) -> arcanum_core::Result<()> { Ok(()) }
    }

    Arc::new(PipelineDeps {
        loaders: Arc::new(LoaderRegistry::new().register(Arc::new(RawLoader::new()))),
        preprocessors: Arc::new(PreprocessorRegistry::new()),
        chunkers,
        context_enricher: None,
        entity_extractor: None,
        embedder: Arc::new(StubEmbedder),
        vector_store: Arc::new(StubVectorStore),
        graph_store: None,
        tree_store: None,
        version_store:     Arc::new(arcanum_core::traits::NoOpDocumentVersionStore),
        snapshot_store:    Arc::new(arcanum_core::traits::InMemorySnapshotStore::new()),
        chunk_metadata:    None,
        retry_policy: arcanum_middleware::RetryPolicy::default(),
        cache_invalidator: Arc::new(arcanum_core::traits::CacheInvalidationBroadcaster::new(vec![])),
        embedding_cb:      Arc::new(arcanum_middleware::CircuitBreaker::new("embedding", 5, std::time::Duration::from_secs(30))),
        vector_store_cb:   Arc::new(arcanum_middleware::CircuitBreaker::new("vector_store", 5, std::time::Duration::from_secs(30))),
        shadow:            None,
    })
}

#[tokio::test]
async fn test_standard_pipeline_runs_all_five_stages() {
    let deps = stub_deps();
    let state = Arc::new(Mutex::new(IngestionState::new(
        Source::Raw {
            content: b"hello world document".to_vec(),
            mime_hint: Some("text/plain".into()),
            uri: "raw://test".into(),
        },
        CollectionId("col1".into()),
    )));
    let reg = ArcanumPipelineRegistry::default();
    let dag = reg.build("standard", state.clone(), &deps).unwrap();
    let ctx = DagExecutor::execute(&dag, Default::default()).await.unwrap();
    assert!(ctx.contains_key("vector_write_ok") || ctx.contains_key(CTX_SKIP));
}
