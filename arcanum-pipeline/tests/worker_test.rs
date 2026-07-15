use arcanum_pipeline::PipelineDeps;
use std::sync::Arc;

fn stub_deps() -> Arc<PipelineDeps> {
    use arcanum_ingestion::{LoaderRegistry, RawLoader};
    use arcanum_core::traits::{Chunker, Embedder, Preprocessor, VectorStore};
    use arcanum_core::types::{*, PerBackendChunkers};
    use async_trait::async_trait;

    struct StubChunker;
    #[async_trait]
    impl Chunker for StubChunker {
        async fn chunk(&self, _doc: &RawDocument) -> arcanum_core::Result<Vec<Chunk>> { Ok(vec![]) }
    }
    struct StubPreprocessor;
    #[async_trait]
    impl Preprocessor for StubPreprocessor {
        async fn process(&self, doc: RawDocument) -> arcanum_core::Result<RawDocument> { Ok(doc) }
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
        preprocessors: Some(Arc::new(StubPreprocessor)),
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
        bm25_index:        None,
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
    use arcanum_ingestion::LoaderRegistry;
    use arcanum_ingestion::RawLoader;
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
    struct StubPreprocessor2;
    #[async_trait::async_trait]
    impl arcanum_core::traits::Preprocessor for StubPreprocessor2 {
        async fn process(&self, doc: arcanum_core::types::RawDocument) -> arcanum_core::Result<arcanum_core::types::RawDocument> { Ok(doc) }
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
        preprocessors:     Some(Arc::new(StubPreprocessor2)),
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
        chunk_metadata:    None,
        bm25_index:        None,
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

#[tokio::test]
async fn test_worker_invalidates_cache_on_genuine_content_change_without_force() {
    use arcanum_pipeline::{ArcanumPipelineRegistry, worker::run_task, PipelineDeps};
    use arcanum_core::traits::{ProgressEmitter, CacheInvalidator, DocumentVersionStore};
    use arcanum_core::types::{CollectionId, DocumentEntry, DocumentId, DocumentVersion, IngestionTask, OperationId, PerBackendChunkers, VersioningPolicy};
    use arcanum_middleware::{BoundedQueue, CircuitBreaker, RetryPolicy};
    use arcanum_ingestion::LoaderRegistry;
    use arcanum_ingestion::RawLoader;
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

    // Reports a previously-stored version with a content_hash that will
    // never match the freshly-loaded (empty) raw:// content, so dedup sets
    // __replace — a genuine content change, not a force re-ingest.
    struct StaleHashVersionStore;
    #[async_trait::async_trait]
    impl DocumentVersionStore for StaleHashVersionStore {
        async fn get_latest(&self, _: &str, _: &str) -> arcanum_core::Result<Option<DocumentVersion>> {
            Ok(Some(DocumentVersion {
                document_id: DocumentId::new(),
                version_num: 1,
                source_uri: "raw://test-changed".into(),
                collection_id: "col1".into(),
                content_hash: "stale-hash-that-will-never-match".into(),
                snapshot_uri: "mem://test".into(),
                canonical_uri: None,
                mime_type: "text/plain".into(),
                status: arcanum_core::types::VersionStatus::Active,
                ingested_at: chrono::Utc::now(),
                extra: std::collections::HashMap::new(),
            }))
        }
        async fn add_version(&self, _: DocumentVersion) -> arcanum_core::Result<()> { Ok(()) }
        async fn supersede_active(&self, _: &DocumentId) -> arcanum_core::Result<()> { Ok(()) }
        async fn list_versions(&self, _: &DocumentId) -> arcanum_core::Result<Vec<DocumentVersion>> { Ok(vec![]) }
        async fn get_versioning_policy(&self, _: &str) -> arcanum_core::Result<VersioningPolicy> { Ok(VersioningPolicy::Replace) }
        async fn set_versioning_policy(&self, _: &str, _: VersioningPolicy) -> arcanum_core::Result<()> { Ok(()) }
        async fn delete_by_source_uri(&self, _: &str, _: &str) -> arcanum_core::Result<()> { Ok(()) }
        async fn get_version(&self, _: &DocumentId, _: u32) -> arcanum_core::Result<Option<DocumentVersion>> { Ok(None) }
        async fn list_collections(&self) -> arcanum_core::Result<Vec<String>> { Ok(vec![]) }
        async fn list_documents(&self, _: &str) -> arcanum_core::Result<Vec<DocumentEntry>> { Ok(vec![]) }
    }

    let call_count = Arc::new(AtomicUsize::new(0));
    let broadcaster = Arc::new(arcanum_core::traits::CacheInvalidationBroadcaster::new(
        vec![Arc::new(CountingInvalidator(call_count.clone()))]
    ));

    struct StubChunker3;
    #[async_trait::async_trait]
    impl arcanum_core::traits::Chunker for StubChunker3 {
        async fn chunk(&self, _: &arcanum_core::types::RawDocument) -> arcanum_core::Result<Vec<arcanum_core::types::Chunk>> { Ok(vec![]) }
    }
    struct StubPreprocessor3;
    #[async_trait::async_trait]
    impl arcanum_core::traits::Preprocessor for StubPreprocessor3 {
        async fn process(&self, doc: arcanum_core::types::RawDocument) -> arcanum_core::Result<arcanum_core::types::RawDocument> { Ok(doc) }
    }
    struct StubEmbedder3;
    #[async_trait::async_trait]
    impl arcanum_core::traits::Embedder for StubEmbedder3 {
        async fn embed(&self, _: Vec<String>) -> arcanum_core::Result<Vec<arcanum_core::types::Vector>> { Ok(vec![]) }
        fn dimension(&self) -> usize { 3 }
    }
    struct StubVectorStore3;
    #[async_trait::async_trait]
    impl arcanum_core::traits::VectorStore for StubVectorStore3 {
        async fn upsert(&self, _: &str, _: Vec<arcanum_core::types::IndexedChunk>) -> arcanum_core::Result<()> { Ok(()) }
        async fn search(&self, _: &str, _: &arcanum_core::traits::VectorQuery) -> arcanum_core::Result<Vec<arcanum_core::traits::ScoredChunk>> { Ok(vec![]) }
        async fn delete(&self, _: &str, _: &[arcanum_core::types::ChunkId]) -> arcanum_core::Result<()> { Ok(()) }
        async fn collection_exists(&self, _: &str) -> arcanum_core::Result<bool> { Ok(true) }
        async fn delete_by_source_uri(&self, _: &str, _: &str) -> arcanum_core::Result<()> { Ok(()) }
    }

    let deps = Arc::new(PipelineDeps {
        loaders:           Arc::new(LoaderRegistry::new().register(Arc::new(RawLoader::new()))),
        preprocessors:     Some(Arc::new(StubPreprocessor3)),
        chunkers: PerBackendChunkers {
            vector: Arc::new(StubChunker3),
            graph:  Arc::new(StubChunker3),
            tree:   Arc::new(StubChunker3),
        },
        context_enricher:  None,
        entity_extractor:  None,
        embedder:          Arc::new(StubEmbedder3),
        vector_store:      Arc::new(StubVectorStore3),
        graph_store:       None,
        tree_store:        None,
        version_store:     Arc::new(StaleHashVersionStore),
        snapshot_store:    Arc::new(arcanum_core::traits::InMemorySnapshotStore::new()),
        chunk_metadata:    None,
        bm25_index:        None,
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
        source_uri: "raw://test-changed".into(),
        collection_id: CollectionId("col1".into()),
        pipeline_template: "standard".into(),
        attempt: 0,
        force: false,
        content: None,
        mime_hint: None,
    };

    run_task(task, registry, deps, Arc::new(NoopEmitter), queue).await.unwrap();
    assert_eq!(call_count.load(Ordering::SeqCst), 1,
        "invalidation should fire once for a genuine content change, not just force");
}

#[tokio::test]
async fn test_worker_fails_when_no_preprocessor_configured() {
    use arcanum_pipeline::{ArcanumPipelineRegistry, worker::run_task, PipelineDeps};
    use arcanum_core::traits::ProgressEmitter;
    use arcanum_core::types::{CollectionId, IngestionTask, OperationId};
    use arcanum_middleware::BoundedQueue;

    struct NoopEmitter;
    #[async_trait::async_trait]
    impl ProgressEmitter for NoopEmitter {
        async fn emit(&self, _: &str, _: serde_json::Value) {}
    }

    let base = stub_deps();
    let deps = Arc::new(PipelineDeps {
        loaders:           base.loaders.clone(),
        preprocessors:     None,
        chunkers:          base.chunkers.clone(),
        shadow:            None,
        context_enricher:  base.context_enricher.clone(),
        entity_extractor:  base.entity_extractor.clone(),
        embedder:          base.embedder.clone(),
        vector_store:      base.vector_store.clone(),
        graph_store:       base.graph_store.clone(),
        tree_store:        base.tree_store.clone(),
        version_store:     base.version_store.clone(),
        snapshot_store:    base.snapshot_store.clone(),
        chunk_metadata:    base.chunk_metadata.clone(),
        bm25_index:        base.bm25_index.clone(),
        retry_policy:      base.retry_policy.clone(),
        cache_invalidator: base.cache_invalidator.clone(),
        embedding_cb:      base.embedding_cb.clone(),
        vector_store_cb:   base.vector_store_cb.clone(),
    });

    let registry = Arc::new(ArcanumPipelineRegistry::default());
    let queue = Arc::new(BoundedQueue::new("test", 10));
    let task = IngestionTask {
        operation_id: OperationId::new(),
        source_uri: "raw://test-no-preprocessor".into(),
        collection_id: CollectionId("col1".into()),
        pipeline_template: "standard".into(),
        attempt: 0,
        force: false,
        content: None,
        mime_hint: None,
    };

    let result = run_task(task, registry, deps, Arc::new(NoopEmitter), queue).await;
    assert!(result.is_err(), "task should fail when no preprocessor is configured");
    let err_str = result.unwrap_err().to_string();
    assert!(err_str.contains("no preprocessor configured"), "unexpected error: {}", err_str);
}
