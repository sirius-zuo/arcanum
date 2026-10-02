//! Restart durability for `IngestionService`.
//!
//! Plan Task: a durable operation must survive an Arcanum restart. A NEW
//! service built over the SAME `SqliteOperationStore` (and the same durable
//! payload store) must still resolve the SAME operation id — by id and by
//! idempotency key — with the logical source URI intact and `mime_hint`
//! absent; an identical replay must NOT enqueue a second task; and, once a
//! worker processes the single task, the terminal `Succeeded` report must
//! carry a concrete original-content URI.

use arcanum_core::traits::{OperationPayloadStore, OperationStore, ProgressEmitter};
use arcanum_core::types::{CollectionId, IngestionSubmission, IngestionTask, OperationStatus};
use arcanum_engine::audit::AuditLogger;
use arcanum_engine::event_bus::EventBus;
use arcanum_engine::services::ingestion::IngestionService;
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
        async fn delete(
            &self,
            _: &str,
            _: &[arcanum_core::types::ChunkId],
        ) -> arcanum_core::Result<()> {
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

fn submission(idempotency_key: &str, logical_source_uri: &str) -> IngestionSubmission {
    IngestionSubmission {
        idempotency_key: idempotency_key.to_string(),
        logical_source_uri: logical_source_uri.to_string(),
        // MIME is deliberately absent: a submission without a hint must stay
        // queryable, intact and `None`, across the restart.
        mime_hint: None,
        collection_id: CollectionId("col-restart".to_string()),
        pipeline_configuration: serde_json::json!({ "template": "standard" }),
        payload: Some(b"durable inline content".to_vec()),
        payload_locator: None,
    }
}

#[tokio::test]
async fn restart_between_submission_and_query_preserves_durable_operation() {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = Arc::new(
        SqliteOperationStore::open(
            dir.path()
                .join("restart-durability.db")
                .to_str()
                .expect("utf8 path"),
        )
        .await
        .expect("open store"),
    );
    let payload_store: Arc<dyn OperationPayloadStore> =
        Arc::new(LocalOperationPayloadStore::new(dir.path().join("payloads")));

    let idempotency_key = "restart-idem-1".to_string();
    let logical_source_uri = "raw://restart-doc".to_string();

    // First service (before the simulated restart).
    let queue1 = Arc::new(BoundedQueue::new("ingestion", 16));
    let service1 = IngestionService::new_from_parts(
        queue1.clone(),
        Arc::new(EventBus::new()),
        Arc::new(AuditLogger::new()),
        store.clone(),
        payload_store.clone(),
    );

    // 1. Submit: new operation, non-nil id, inline payload staged durably.
    let (op_id, is_new) = service1
        .submit_operation(
            submission(&idempotency_key, &logical_source_uri),
            false,
            "user-1",
        )
        .await
        .expect("first submit");
    assert!(!op_id.0.is_nil(), "operation id must be non-nil");
    assert!(is_new, "first submission must be new");

    // 2. Accepted + logical source URI verbatim + MIME absent.
    let accepted = store
        .get(&op_id)
        .await
        .expect("get accepted")
        .expect("operation exists");
    assert_eq!(accepted.status, OperationStatus::Accepted);
    assert_eq!(
        accepted.submission.logical_source_uri, logical_source_uri,
        "logical source URI must be stored verbatim"
    );
    assert!(
        accepted.submission.mime_hint.is_none(),
        "MIME may be absent and must stay None"
    );

    // 3. Idempotent replay: the identical submission returns the SAME id and
    //    must NOT enqueue a second task.
    let (replay_id, replay_is_new) = service1
        .submit_operation(
            submission(&idempotency_key, &logical_source_uri),
            false,
            "user-1",
        )
        .await
        .expect("replay");
    assert_eq!(
        replay_id, op_id,
        "replay must return the original operation id"
    );
    assert!(!replay_is_new, "replay must not create a new operation");

    // 4. Restart: drop the first service and its in-memory queue (losing the
    //    pending task), then build a NEW service over the SAME durable store.
    drop(service1);
    drop(queue1);
    let queue2 = Arc::new(BoundedQueue::new("ingestion", 16));
    let service2 = IngestionService::new_from_parts(
        queue2.clone(),
        Arc::new(EventBus::new()),
        Arc::new(AuditLogger::new()),
        store.clone(),
        payload_store.clone(),
    );

    // 5. Query after restart: by id and by idempotency key.
    let by_id = service2
        .operations()
        .get(&op_id)
        .await
        .expect("get by id after restart")
        .expect("operation exists after restart");
    assert_eq!(by_id.status, OperationStatus::Accepted);
    assert_eq!(by_id.submission.logical_source_uri, logical_source_uri);
    assert!(by_id.submission.mime_hint.is_none());

    let by_key = service2
        .operations()
        .get_by_idempotency(&idempotency_key)
        .await
        .expect("get by idempotency after restart")
        .expect("operation exists by idempotency after restart");
    assert_eq!(
        by_key.operation_id, op_id,
        "idempotency lookup must resolve the original operation"
    );
    assert_eq!(by_key.submission.logical_source_uri, logical_source_uri);
    assert!(by_key.submission.mime_hint.is_none());

    // 6. Process the single task after the restart. The durable Accepted
    //    operation carries everything needed to resume work: reconstruct the
    //    task from the stored submission (the worker resolves the inline
    //    content from the staged `payload_locator`) and run it over the fresh
    //    queue.
    let stored = by_id.submission;
    let payload_locator = stored
        .payload_locator
        .clone()
        .expect("inline payload must be staged durably");
    let task = IngestionTask {
        operation_id: op_id.clone(),
        source_uri: stored.logical_source_uri.clone(),
        collection_id: stored.collection_id.clone(),
        pipeline_template: stored
            .pipeline_configuration
            .get("template")
            .and_then(|v| v.as_str())
            .unwrap_or("standard")
            .to_string(),
        force: false,
        content: None,
        mime_hint: stored.mime_hint.clone(),
        payload_locator: Some(payload_locator),
    };
    queue2.push(task).await.expect("enqueue the single task");

    let worker = IngestionWorker::new(
        Arc::new(ArcanumPipelineRegistry::default()),
        stub_deps(),
        noop_emitter(),
        queue2.clone(),
        store.clone(),
        Some(payload_store.clone()),
    );
    let processed = worker.process_next().await.expect("one task was enqueued");
    processed.expect("task processed cleanly");

    // Terminal report is durable and carries a concrete original-content URI.
    let terminal = store
        .get(&op_id)
        .await
        .expect("get terminal")
        .expect("operation exists");
    assert_eq!(terminal.status, OperationStatus::Succeeded);
    let report = terminal.terminal_report.expect("terminal report persisted");
    let content_uri = report
        .content_uri
        .expect("successful report must carry the original-content URI");
    assert!(
        !content_uri.is_empty(),
        "content_uri must be the concrete original-content URI"
    );
}
