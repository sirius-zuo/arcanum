use arcanum_pipeline::PipelineDeps;
use std::sync::Arc;

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
        retry_policy: arcanum_middleware::RetryPolicy::default(),
        cache_invalidator: Arc::new(arcanum_core::traits::CacheInvalidationBroadcaster::new(vec![])),
        embedding_cb:      Arc::new(arcanum_middleware::CircuitBreaker::new("embedding", 5, std::time::Duration::from_secs(30))),
        vector_store_cb:   Arc::new(arcanum_middleware::CircuitBreaker::new("vector_store", 5, std::time::Duration::from_secs(30))),
        shadow:            None,
    })
}

#[tokio::test]
async fn test_worker_processes_task_to_completion() {
    use arcanum_pipeline::{ArcanumPipelineRegistry, worker::run_task};
    use arcanum_core::traits::ProgressEmitter;
    use arcanum_core::types::{CollectionId, IngestionTask, OperationId};
    use arcanum_middleware::BoundedQueue;
    use std::sync::Arc;

    struct NoopEmitter;
    #[async_trait::async_trait]
    impl ProgressEmitter for NoopEmitter {
        async fn emit(&self, _: &str, _: serde_json::Value) {}
    }

    let deps = stub_deps();
    let registry = Arc::new(ArcanumPipelineRegistry::default());
    let queue = Arc::new(BoundedQueue::new("test", 10));

    let task = IngestionTask {
        operation_id: OperationId::new(),
        source_uri: "raw://test".into(),
        collection_id: CollectionId("col1".into()),
        pipeline_template: "standard".into(),
        attempt: 0,
        force: false,
        content: None,
        mime_hint: None,
    };

    let result = run_task(task, registry, deps, Arc::new(NoopEmitter), queue).await;
    assert!(result.is_ok(), "worker task failed: {:?}", result.err());
}

#[tokio::test]
async fn test_embed_stage_blocked_by_open_circuit_breaker() {
    use arcanum_pipeline::{ArcanumPipelineRegistry, worker::run_task};
    use arcanum_core::traits::ProgressEmitter;
    use arcanum_core::types::{CollectionId, IngestionTask, OperationId};
    use arcanum_middleware::BoundedQueue;

    struct NoopEmitter;
    #[async_trait::async_trait]
    impl ProgressEmitter for NoopEmitter {
        async fn emit(&self, _: &str, _: serde_json::Value) {}
    }

    let deps = stub_deps();
    // Trip the embedding circuit breaker past its threshold (5 failures).
    for _ in 0..5 {
        deps.embedding_cb.record_failure();
    }
    assert!(!deps.embedding_cb.allow_request(), "circuit should be open");

    let registry = Arc::new(ArcanumPipelineRegistry::default());
    let queue = Arc::new(BoundedQueue::new("test", 10));
    let task = IngestionTask {
        operation_id: OperationId::new(),
        source_uri: "raw://test-cb".into(),
        collection_id: CollectionId("col1".into()),
        pipeline_template: "standard".into(),
        attempt: 0,
        force: false,
        content: None,
        mime_hint: None,
    };

    let result = run_task(task, registry, deps, Arc::new(NoopEmitter), queue).await;
    assert!(result.is_err(), "open circuit breaker should cause task failure");
    let err_str = result.unwrap_err().to_string();
    assert!(err_str.contains("circuit"), "error should mention circuit: {}", err_str);
}

#[tokio::test]
async fn test_worker_invalidates_cache_on_force_reingest() {
    use arcanum_pipeline::{ArcanumPipelineRegistry, worker::run_task, PipelineDeps};
    use arcanum_core::traits::{ProgressEmitter, CacheInvalidator};
    use arcanum_core::types::{CollectionId, IngestionTask, OperationId, PerBackendChunkers};
    use arcanum_middleware::{BoundedQueue, CircuitBreaker, RetryPolicy};
    use arcanum_ingestion::{LoaderRegistry, PreprocessorRegistry, RawLoader};
    use std::time::Duration;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct NoopEmitter;
    #[async_trait::async_trait]
    impl ProgressEmitter for NoopEmitter {
        async fn emit(&self, _: &str, _: serde_json::Value) {}
    }

    struct CountingInvalidator(Arc<AtomicUsize>);
    #[async_trait::async_trait]
    impl CacheInvalidator for CountingInvalidator {
        async fn invalidate_document(&self, _uri: &str, _col: &arcanum_core::types::CollectionId) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    let call_count = Arc::new(AtomicUsize::new(0));
    let broadcaster = Arc::new(arcanum_core::traits::CacheInvalidationBroadcaster::new(
        vec![Arc::new(CountingInvalidator(call_count.clone()))]
    ));

    struct StubChunker2;
    #[async_trait::async_trait]
    impl arcanum_core::traits::Chunker for StubChunker2 {
        async fn chunk(&self, _: &arcanum_core::types::RawDocument) -> arcanum_core::Result<Vec<arcanum_core::types::Chunk>> { Ok(vec![]) }
    }
    struct StubEmbedder2;
    #[async_trait::async_trait]
    impl arcanum_core::traits::Embedder for StubEmbedder2 {
        async fn embed(&self, _: Vec<String>) -> arcanum_core::Result<Vec<arcanum_core::types::Vector>> { Ok(vec![]) }
        fn dimension(&self) -> usize { 3 }
    }
    struct StubVectorStore2;
    #[async_trait::async_trait]
    impl arcanum_core::traits::VectorStore for StubVectorStore2 {
        async fn upsert(&self, _: &str, _: Vec<arcanum_core::types::IndexedChunk>) -> arcanum_core::Result<()> { Ok(()) }
        async fn search(&self, _: &str, _: &arcanum_core::traits::VectorQuery) -> arcanum_core::Result<Vec<arcanum_core::traits::ScoredChunk>> { Ok(vec![]) }
        async fn delete(&self, _: &str, _: &[arcanum_core::types::ChunkId]) -> arcanum_core::Result<()> { Ok(()) }
        async fn collection_exists(&self, _: &str) -> arcanum_core::Result<bool> { Ok(true) }
        async fn delete_by_source_uri(&self, _: &str, _: &str) -> arcanum_core::Result<()> { Ok(()) }
    }

    let deps = Arc::new(PipelineDeps {
        loaders:           Arc::new(LoaderRegistry::new().register(Arc::new(RawLoader::new()))),
        preprocessors:     Arc::new(PreprocessorRegistry::new()),
        chunkers: PerBackendChunkers {
            vector: Arc::new(StubChunker2),
            graph:  Arc::new(StubChunker2),
            tree:   Arc::new(StubChunker2),
        },
        context_enricher:  None,
        entity_extractor:  None,
        embedder:          Arc::new(StubEmbedder2),
        vector_store:      Arc::new(StubVectorStore2),
        graph_store:       None,
        tree_store:        None,
        version_store:     Arc::new(arcanum_core::traits::NoOpDocumentVersionStore),
        snapshot_store:    Arc::new(arcanum_core::traits::InMemorySnapshotStore::new()),
        retry_policy:      RetryPolicy::default(),
        cache_invalidator: broadcaster,
        embedding_cb:      Arc::new(CircuitBreaker::new("embedding", 5, Duration::from_secs(30))),
        vector_store_cb:   Arc::new(CircuitBreaker::new("vector_store", 5, Duration::from_secs(30))),
        shadow:            None,
    });

    let registry = Arc::new(ArcanumPipelineRegistry::default());
    let queue = Arc::new(BoundedQueue::new("test", 10));
    let task = IngestionTask {
        operation_id: OperationId::new(),
        source_uri: "raw://test-force".into(),
        collection_id: CollectionId("col1".into()),
        pipeline_template: "standard".into(),
        attempt: 0,
        force: true,
        content: None,
        mime_hint: None,
    };

    run_task(task, registry, deps, Arc::new(NoopEmitter), queue).await.unwrap();
    assert_eq!(call_count.load(Ordering::SeqCst), 1,
        "invalidation should fire once for force-reingest");
}
