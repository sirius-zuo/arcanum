use arcanum_pipeline::PipelineDeps;
use std::sync::Arc;

fn stub_deps() -> Arc<PipelineDeps> {
    use arcanum_ingestion::{LoaderRegistry, PreprocessorRegistry, DocumentHashTracker, RawLoader};
    use arcanum_core::traits::{Chunker, Embedder, VectorStore};
    use arcanum_core::types::*;
    use async_trait::async_trait;

    struct StubChunker;
    #[async_trait]
    impl Chunker for StubChunker {
        async fn chunk(&self, _doc: &RawDocument) -> arcanum_core::Result<Vec<Chunk>> { Ok(vec![]) }
    }
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
    }

    Arc::new(PipelineDeps {
        loaders: Arc::new(LoaderRegistry::new().register(Arc::new(RawLoader::new()))),
        preprocessors: Arc::new(PreprocessorRegistry::new()),
        chunker: Arc::new(StubChunker),
        context_enricher: None,
        entity_extractor: None,
        embedder: Arc::new(StubEmbedder),
        vector_store: Arc::new(StubVectorStore),
        graph_store: None,
        tree_store: None,
        hash_tracker: Arc::new(DocumentHashTracker::new()),
        retry_policy: arcanum_middleware::RetryPolicy::default(),
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
    let queue = Arc::new(BoundedQueue::new(10));

    let task = IngestionTask {
        operation_id: OperationId::new(),
        source_uri: "raw://test".into(),
        collection_id: CollectionId("col1".into()),
        pipeline_template: "standard".into(),
        attempt: 0,
    };

    let result = run_task(task, registry, deps, Arc::new(NoopEmitter), queue).await;
    assert!(result.is_ok(), "worker task failed: {:?}", result.err());
}

#[tokio::test]
async fn test_worker_skips_unchanged_document() {
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
    // Pre-record the hash so the document appears unchanged
    deps.hash_tracker.record("raw://test", b"hello world document").await;

    let registry = Arc::new(ArcanumPipelineRegistry::default());
    let queue = Arc::new(BoundedQueue::new(10));

    let task = IngestionTask {
        operation_id: OperationId::new(),
        source_uri: "raw://test".into(),
        collection_id: CollectionId("col1".into()),
        pipeline_template: "standard".into(),
        attempt: 0,
    };

    let result = run_task(task, registry, deps, Arc::new(NoopEmitter), queue).await;
    assert!(result.is_ok());
}
