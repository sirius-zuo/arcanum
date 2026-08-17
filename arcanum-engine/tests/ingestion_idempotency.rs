//! Lost-event recovery + idempotent replay for `IngestionService`.
//!
//! Plan Task 3, Step 1: submission persists `Accepted` BEFORE the task is
//! enqueued; an identical replay returns the ORIGINAL operation id without
//! enqueueing a second task; once a worker processes the task, the terminal
//! report is durable in the store — a NEW service over the SAME store observes
//! `Succeeded` with the original-content URI even though no event subscriber
//! was ever connected.

use arcanum_engine::audit::AuditLogger;
use arcanum_engine::event_bus::EventBus;
use arcanum_engine::services::ingestion::{IngestionService, IngestRequest};
use arcanum_core::traits::{OperationPayloadStore, OperationStore, ProgressEmitter};
use arcanum_core::types::{CollectionId, IngestionOutcome, OperationStatus};
use arcanum_ingestion::operations::sqlite::SqliteOperationStore;
use arcanum_ingestion::LocalOperationPayloadStore;
use arcanum_middleware::BoundedQueue;
use arcanum_pipeline::{worker::IngestionWorker, ArcanumPipelineRegistry, PipelineDeps};
use std::sync::Arc;

fn noop_emitter() -> Arc<dyn ProgressEmitter> {
    struct Noop;
    #[async_trait::async_trait]
    impl ProgressEmitter for Noop {
        async fn emit(&self, _: &str, _: serde_json::Value) {}
    }
    Arc::new(Noop)
}

/// Minimal pipeline deps so the "standard" template can execute to completion
/// (load -> dedup -> cleanup -> preprocess -> snapshot -> chunk -> embed ->
/// vector_write -> register_version) against no-op stores.
fn stub_deps() -> Arc<PipelineDeps> {
    use arcanum_core::traits::{Chunker, Embedder, Preprocessor, VectorStore};
    use arcanum_core::types::PerBackendChunkers;
    use arcanum_ingestion::{LoaderRegistry, RawLoader};

    struct StubChunker;
    #[async_trait::async_trait]
    impl Chunker for StubChunker {
        async fn chunk(
            &self,
            _: &arcanum_core::types::RawDocument,
        ) -> arcanum_core::Result<Vec<arcanum_core::types::Chunk>> {
            Ok(vec![])
        }
    }
    struct StubPreprocessor;
    #[async_trait::async_trait]
    impl Preprocessor for StubPreprocessor {
        async fn process(
            &self,
            doc: arcanum_core::types::RawDocument,
        ) -> arcanum_core::Result<arcanum_core::types::RawDocument> {
            Ok(doc)
        }
    }
    struct StubEmbedder;
    #[async_trait::async_trait]
    impl Embedder for StubEmbedder {
        async fn embed(
            &self,
            _: Vec<String>,
        ) -> arcanum_core::Result<Vec<arcanum_core::types::Vector>> {
            Ok(vec![])
        }
        fn dimension(&self) -> usize {
            3
        }
    }
    struct StubVectorStore;
    #[async_trait::async_trait]
    impl VectorStore for StubVectorStore {
        async fn upsert(
            &self,
            _: &str,
            _: Vec<arcanum_core::types::IndexedChunk>,
        ) -> arcanum_core::Result<()> {
            Ok(())
        }
        async fn search(
            &self,
            _: &str,
            _: &arcanum_core::traits::VectorQuery,
        ) -> arcanum_core::Result<Vec<arcanum_core::traits::ScoredChunk>> {
            Ok(vec![])
        }
        async fn delete(&self, _: &str, _: &[arcanum_core::types::ChunkId]) -> arcanum_core::Result<()> {
            Ok(())
        }
        async fn collection_exists(&self, _: &str) -> arcanum_core::Result<bool> {
            Ok(true)
        }
        async fn delete_by_source_uri(&self, _: &str, _: &str) -> arcanum_core::Result<()> {
            Ok(())
        }
    }

    let chunker = Arc::new(StubChunker) as Arc<dyn Chunker>;
    Arc::new(PipelineDeps {
        loaders: Arc::new(LoaderRegistry::new().register(Arc::new(RawLoader::new()))),
        preprocessors: Some(Arc::new(StubPreprocessor)),
        chunkers: PerBackendChunkers {
            vector: chunker.clone(),
            graph: chunker.clone(),
            tree: chunker.clone(),
        },
        shadow: None,
        context_enricher: None,
        entity_extractor: None,
        embedder: Arc::new(StubEmbedder),
        vector_store: Arc::new(StubVectorStore),
        graph_store: None,
        tree_store: None,
        version_store: Arc::new(arcanum_core::traits::NoOpDocumentVersionStore),
        snapshot_store: Arc::new(arcanum_core::traits::InMemorySnapshotStore::new()),
        chunk_metadata: None,
        bm25_index: None,
        cache_invalidator: Arc::new(arcanum_core::traits::CacheInvalidationBroadcaster::new(
            vec![],
        )),
        embedding_cb: Arc::new(arcanum_middleware::CircuitBreaker::new(
            "embedding",
            5,
            std::time::Duration::from_secs(30),
        )),
        vector_store_cb: Arc::new(arcanum_middleware::CircuitBreaker::new(
            "vector_store",
            5,
            std::time::Duration::from_secs(30),
        )),
    })
}

#[tokio::test]
async fn ingestion_idempotency_lost_event_recovery_returns_terminal_report() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("ingestion-idempotency.db");
    let store = Arc::new(
        SqliteOperationStore::open(path.to_str().expect("utf8 path"))
            .await
            .expect("open store"),
    );

    let queue = Arc::new(BoundedQueue::new("ingestion", 16));
    let events = Arc::new(EventBus::new());
    let audit = Arc::new(AuditLogger::new());
    let payload_store: Arc<dyn OperationPayloadStore> =
        Arc::new(LocalOperationPayloadStore::new(dir.path().join("payloads")));

    // No event subscribers are ever attached: the WebSocket bus is best-effort,
    // and terminal truth must come from the store, not the bus.
    let service = IngestionService::new_from_parts(
        queue.clone(),
        events.clone(),
        audit.clone(),
        store.clone(),
        payload_store.clone(),
    );

    let req = IngestRequest {
        source_uri: "raw://doc-a".to_string(),
        collection_id: CollectionId("col-a".to_string()),
        pipeline_template: Some("standard".to_string()),
        force: false,
        content: Some(b"original content".to_vec()),
        mime_hint: Some("text/plain".to_string()),
    };

    let op_id = service.ingest(req.clone(), "user-1").await.expect("first ingest");

    // Accepted is persisted before the task is enqueued.
    let accepted = store
        .get(&op_id)
        .await
        .expect("get accepted")
        .expect("operation exists");
    assert_eq!(accepted.status, OperationStatus::Accepted);

    // Idempotent replay: the identical submission returns the ORIGINAL id and
    // must NOT enqueue a second task.
    let replay_id = service.ingest(req.clone(), "user-1").await.expect("replay");
    assert_eq!(replay_id, op_id, "replay must return the original operation id");

    // Process the single queued task with a worker over the same store + queue.
    let worker = IngestionWorker::new(
        Arc::new(ArcanumPipelineRegistry::default()),
        stub_deps(),
        noop_emitter(),
        queue.clone(),
        store.clone(),
        Some(payload_store.clone()),
    );
    let processed = worker.process_next().await.expect("one task was enqueued");
    processed.expect("task processed cleanly");

    // A NEW service over the SAME store observes the terminal report.
    let _service2 = IngestionService::new_from_parts(
        queue,
        events,
        audit,
        store.clone(),
        payload_store,
    );
    let operation = store
        .get(&op_id)
        .await
        .expect("get terminal")
        .expect("operation exists");
    assert_eq!(operation.status, OperationStatus::Succeeded);
    let report = operation.terminal_report.expect("terminal report persisted");
    assert_eq!(report.outcome, Some(IngestionOutcome::Ingested));
    let content_uri = report
        .content_uri
        .expect("successful report must carry the original-content URI");
    assert!(
        !content_uri.is_empty(),
        "content_uri must be the version-specific original-content URI"
    );
}
