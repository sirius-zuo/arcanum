//! Worker terminal-transition persistence.
//!
//! Plan Task 3, Step 3: the pipeline worker marks the operation `Running`
//! before preprocessing, persists the terminal report (`Succeeded` with the
//! original content URI, `Unchanged` with the existing version's snapshot URI,
//! or `Failed` with a safe, redacted error) BEFORE emitting any event, and the
//! result is durable across a fresh store instance over the same storage. A
//! terminal `Failed` operation is final — the worker never re-enqueues a retry.

use arcanum_core::traits::{DocumentVersionStore, OperationStore, ProgressEmitter};
use arcanum_core::types::{
    CollectionId, DocumentEntry, DocumentId, DocumentVersion, IngestionOutcome,
    IngestionSubmission, IngestionTask, OperationId, OperationStatus, VersioningPolicy,
    VersionStatus,
};
use arcanum_core::ArcanumError;
use arcanum_ingestion::operations::sqlite::SqliteOperationStore;
use arcanum_pipeline::{
    dag::{PipelineDAG, PipelineStage},
    worker::run_task,
    ArcanumPipelineRegistry, PipelineDeps,
};
use std::sync::Arc;

fn noop_emitter() -> Arc<dyn ProgressEmitter> {
    struct Noop;
    #[async_trait::async_trait]
    impl ProgressEmitter for Noop {
        async fn emit(&self, _: &str, _: serde_json::Value) {}
    }
    Arc::new(Noop)
}

/// Minimal pipeline deps so the "standard" template executes to completion
/// against no-op stores (InMemorySnapshotStore produces a `mem://` snapshot
/// URI used as the durable content URI).
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
        retry_policy: arcanum_middleware::RetryPolicy::default(),
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

fn submission(source_uri: &str, key: &str) -> IngestionSubmission {
    IngestionSubmission {
        idempotency_key: key.to_string(),
        logical_source_uri: source_uri.to_string(),
        mime_hint: Some("text/plain".to_string()),
        collection_id: CollectionId("col1".to_string()),
        pipeline_configuration: serde_json::json!({ "template": "standard" }),
        payload: None,
        payload_locator: None,
    }
}

fn task(op_id: OperationId, source_uri: &str, content: Option<&[u8]>) -> IngestionTask {
    IngestionTask {
        operation_id: op_id,
        source_uri: source_uri.to_string(),
        collection_id: CollectionId("col1".to_string()),
        pipeline_template: "standard".to_string(),
        attempt: 0,
        force: false,
        content: content.map(|b| b.to_vec()),
        mime_hint: Some("text/plain".to_string()),
        payload_locator: None,
    }
}

async fn open_store(dir: &tempfile::TempDir, name: &str) -> Arc<SqliteOperationStore> {
    let path = dir.path().join(name);
    Arc::new(
        SqliteOperationStore::open(path.to_str().expect("utf8 path"))
            .await
            .expect("open store"),
    )
}

#[tokio::test]
async fn durable_completion_new_ingestion_persists_succeeded_with_content_uri() {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = open_store(&dir, "new.db").await;
    let created = store
        .create_or_get(&submission("raw://doc-b", "idem-new-1"))
        .await
        .expect("create_or_get");
    assert!(created.is_new);
    let op_id = created.operation.operation_id.clone();

    run_task(
        task(op_id.clone(), "raw://doc-b", Some(b"durable content")),
        Arc::new(ArcanumPipelineRegistry::default()),
        stub_deps(),
        noop_emitter(),
        store.clone(),
        None,
    )
    .await
    .expect("run_task");

    // The store holds the terminal report, even through a FRESH store instance
    // over the same storage (restart recovery).
    let fresh = open_store(&dir, "new.db").await;
    let op = fresh
        .get(&op_id)
        .await
        .expect("get terminal")
        .expect("operation exists");
    assert_eq!(op.status, OperationStatus::Succeeded);
    let report = op.terminal_report.expect("terminal report persisted");
    assert_eq!(report.outcome, Some(IngestionOutcome::Ingested));
    let uri = report.content_uri.expect("content_uri must be present");
    assert!(
        uri.starts_with("mem://"),
        "content_uri should be the pipeline snapshot URI, got: {uri}"
    );
}

/// A version store that reports a pre-existing active version whose
/// content_hash matches the content the pipeline loads, so dedup sets
/// `CTX_SKIP` and the worker must return the EXISTING snapshot URI.
struct FixedVersionStore {
    latest: DocumentVersion,
}

#[async_trait::async_trait]
impl DocumentVersionStore for FixedVersionStore {
    async fn get_latest(&self, _: &str, _: &str) -> arcanum_core::Result<Option<DocumentVersion>> {
        Ok(Some(self.latest.clone()))
    }
    async fn add_version(&self, _: DocumentVersion) -> arcanum_core::Result<()> {
        Ok(())
    }
    async fn supersede_active(&self, _: &DocumentId) -> arcanum_core::Result<()> {
        Ok(())
    }
    async fn list_versions(&self, _: &DocumentId) -> arcanum_core::Result<Vec<DocumentVersion>> {
        Ok(vec![])
    }
    async fn get_versioning_policy(&self, _: &str) -> arcanum_core::Result<VersioningPolicy> {
        Ok(VersioningPolicy::Replace)
    }
    async fn set_versioning_policy(&self, _: &str, _: VersioningPolicy) -> arcanum_core::Result<()> {
        Ok(())
    }
    async fn delete_by_source_uri(&self, _: &str, _: &str) -> arcanum_core::Result<()> {
        Ok(())
    }
    async fn list_collections(&self) -> arcanum_core::Result<Vec<String>> {
        Ok(vec![])
    }
    async fn get_version(&self, _: &DocumentId, _: u32) -> arcanum_core::Result<Option<DocumentVersion>> {
        Ok(None)
    }
    async fn list_documents(&self, _: &str) -> arcanum_core::Result<Vec<DocumentEntry>> {
        Ok(vec![])
    }
}

#[tokio::test]
async fn durable_completion_unchanged_content_returns_existing_snapshot_uri() {
    let content = b"stable content";
    let expected_hash =
        arcanum_core::types::RawDocument::for_test(content.to_vec(), "text/plain").content_hash();
    let existing = DocumentVersion {
        document_id: DocumentId::new(),
        version_num: 1,
        source_uri: "raw://doc-c".to_string(),
        collection_id: "col1".to_string(),
        content_hash: expected_hash,
        snapshot_uri: "s3://arcanum/existing/version-1".to_string(),
        canonical_uri: None,
        mime_type: "text/plain".to_string(),
        status: VersionStatus::Active,
        ingested_at: chrono::Utc::now(),
        extra: Default::default(),
    };

    let dir = tempfile::tempdir().expect("tempdir");
    let store = open_store(&dir, "unchanged.db").await;
    let created = store
        .create_or_get(&submission("raw://doc-c", "idem-unchanged"))
        .await
        .expect("create_or_get");
    let op_id = created.operation.operation_id.clone();

    let deps = {
        let base = stub_deps();
        Arc::new(PipelineDeps {
            loaders: base.loaders.clone(),
            preprocessors: base.preprocessors.clone(),
            chunkers: base.chunkers.clone(),
            shadow: None,
            context_enricher: base.context_enricher.clone(),
            entity_extractor: base.entity_extractor.clone(),
            embedder: base.embedder.clone(),
            vector_store: base.vector_store.clone(),
            graph_store: base.graph_store.clone(),
            tree_store: base.tree_store.clone(),
            version_store: Arc::new(FixedVersionStore {
                latest: existing.clone(),
            }),
            snapshot_store: base.snapshot_store.clone(),
            chunk_metadata: base.chunk_metadata.clone(),
            bm25_index: base.bm25_index.clone(),
            retry_policy: base.retry_policy.clone(),
            cache_invalidator: base.cache_invalidator.clone(),
            embedding_cb: base.embedding_cb.clone(),
            vector_store_cb: base.vector_store_cb.clone(),
        })
    };

    run_task(
        task(op_id.clone(), "raw://doc-c", Some(content)),
        Arc::new(ArcanumPipelineRegistry::default()),
        deps,
        noop_emitter(),
        store.clone(),
        None,
    )
    .await
    .expect("run_task");

    let op = store
        .get(&op_id)
        .await
        .expect("get terminal")
        .expect("operation exists");
    assert_eq!(op.status, OperationStatus::Succeeded);
    let report = op.terminal_report.expect("terminal report persisted");
    assert_eq!(report.outcome, Some(IngestionOutcome::Unchanged));
    assert_eq!(
        report.content_uri.as_deref(),
        Some("s3://arcanum/existing/version-1"),
        "unchanged content must return the existing version's snapshot URI"
    );
}

#[tokio::test]
async fn durable_completion_worker_persists_failed_with_safe_error_and_disposition() {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = open_store(&dir, "failed.db").await;
    let created = store
        .create_or_get(&submission("raw://doc-d", "idem-failed"))
        .await
        .expect("create_or_get");
    let op_id = created.operation.operation_id.clone();

    let deps = {
        let base = stub_deps();
        // Trip the embedding circuit breaker past its threshold so the core
        // "embed" stage fails and the pipeline aborts.
        for _ in 0..5 {
            base.embedding_cb.record_failure();
        }
        base
    };

    let result = run_task(
        task(op_id.clone(), "raw://doc-d", Some(b"boom content")),
        Arc::new(ArcanumPipelineRegistry::default()),
        deps,
        noop_emitter(),
        store.clone(),
        None,
    )
    .await;
    assert!(result.is_err(), "open circuit breaker should fail the task");

    let op = store
        .get(&op_id)
        .await
        .expect("get terminal")
        .expect("operation exists");
    assert_eq!(op.status, OperationStatus::Failed);
    let report = op.terminal_report.expect("terminal report persisted");
    let err = report.error.expect("failed report must carry a safe error");
    assert_eq!(err.code, "EMBEDDING_FAILURE");
    assert!(
        err.retryable,
        "an embedding outage is transient and must be marked retryable"
    );
    assert_eq!(
        report.partial_output_disposition, None,
        "failed report has no cleanup mechanism, so the disposition must be honest None"
    );
}

/// A terminal `Failed` operation must NOT be re-enqueued for retry: once the
/// failure is persisted, the worker no longer pushes a retry back onto the
/// queue, and the durable store refuses any re-processing transition
/// (`Accepted -> Running`). The operation stays terminal `Failed`.
#[tokio::test]
async fn durable_completion_failed_operation_is_terminal_and_not_re_enqueued() {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = open_store(&dir, "terminal.db").await;
    let created = store
        .create_or_get(&submission("raw://doc-e", "idem-terminal"))
        .await
        .expect("create_or_get");
    let op_id = created.operation.operation_id.clone();

    // Force a deterministic core failure: open the embedding circuit breaker
    // so the "embed" stage errors and the pipeline aborts.
    let deps = {
        let base = stub_deps();
        for _ in 0..5 {
            base.embedding_cb.record_failure();
        }
        base
    };

    let result = run_task(
        task(op_id.clone(), "raw://doc-e", Some(b"boom")),
        Arc::new(ArcanumPipelineRegistry::default()),
        deps,
        noop_emitter(),
        store.clone(),
        None,
    )
    .await;
    assert!(result.is_err(), "open circuit breaker should fail the task");

    // Terminal Failed is persisted.
    let op = store.get(&op_id).await.expect("get").expect("op exists");
    assert_eq!(op.status, OperationStatus::Failed);
    assert!(op.terminal_report.is_some(), "terminal report must be persisted");

    // A retry-shaped re-run (the old worker re-enqueue path) is rejected by
    // the durable guard: a terminal Failed cannot transition back to Running.
    let mark_err = store.mark_running(&op_id, chrono::Utc::now()).await;
    assert!(mark_err.is_err(), "terminal Failed must not be markable Running");
    let op = store.get(&op_id).await.expect("get").expect("op exists");
    assert_eq!(
        op.status,
        OperationStatus::Failed,
        "operation must remain terminal Failed after a rejected re-run"
    );
}

/// A failed pipeline whose underlying error embeds a connection URL, a bare
/// `host:port` fragment, a hostname, and a filesystem path must persist a
/// FULLY GENERIC message: the durable `SafeOperationError.message` is served
/// verbatim over the query API (Task 4), so the worker must never reproduce
/// ANY of the underlying error's text — only the stable code prefix.
#[tokio::test]
async fn durable_completion_failed_message_sanitizes_url_and_path() {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = open_store(&dir, "sanitized.db").await;
    let created = store
        .create_or_get(&submission("raw://doc-f", "idem-sanitized"))
        .await
        .expect("create_or_get");
    let op_id = created.operation.operation_id.clone();

    // A core "load" stage that fails with a Storage error embedding a URL, a
    // bare `host:port`, and a filesystem path — the exact shape of
    // sqlx/reqwest failures.
    let mut registry = ArcanumPipelineRegistry::new();
    registry.register("failing_load", Arc::new(|_state, _deps| {
        PipelineDAG::new().add_stage(PipelineStage {
            id: "load",
            deps: vec![],
            run: Arc::new(|_ctx| {
                Box::pin(async move {
                    Err(ArcanumError::Storage(
                        "load failed: GET https://data.example.com/private/doc.pdf \
                         -> connection refused at db.example.com:5432; \
                         temp file /var/lib/arcanum/cache/x.db"
                            .to_string(),
                    ))
                })
            }),
        })
    }));
    let registry = Arc::new(registry);

    let result = run_task(
        IngestionTask {
            operation_id: op_id.clone(),
            source_uri: "raw://doc-f".into(),
            collection_id: CollectionId("col1".into()),
            pipeline_template: "failing_load".into(),
            attempt: 0,
            force: false,
            content: Some(b"boom".to_vec()),
            mime_hint: Some("text/plain".to_string()),
            payload_locator: None,
        },
        registry,
        stub_deps(),
        noop_emitter(),
        store.clone(),
        None,
    )
    .await;
    assert!(result.is_err(), "core load failure should fail the task");

    let op = store.get(&op_id).await.expect("get").expect("op exists");
    assert_eq!(op.status, OperationStatus::Failed);
    let report = op.terminal_report.expect("terminal report persisted");
    let err = report.error.expect("failed report must carry a safe error");
    assert_eq!(err.code, "STORAGE_FAILURE");
    assert!(!err.retryable, "a storage failure is not transient");

    let msg = &err.message;
    // The message keeps the stable code prefix…
    assert!(
        msg.contains("STORAGE_FAILURE"),
        "persisted message must keep the stable error code prefix: {msg}"
    );
    // …but reproduces NONE of the underlying error's text: no URL, hostname,
    // bare `host:port`, or filesystem path.
    assert!(
        !msg.contains("https://"),
        "persisted message must not contain the connection URL: {msg}"
    );
    assert!(
        !msg.contains("data.example.com"),
        "persisted message must not contain the hostname: {msg}"
    );
    assert!(
        !msg.contains("db.example.com") && !msg.contains("5432"),
        "persisted message must not contain the bare host:port fragment: {msg}"
    );
    assert!(
        !msg.contains("/var/lib/arcanum/cache") && !msg.contains("x.db"),
        "persisted message must not contain the filesystem path: {msg}"
    );
    // The message IS the generic safe phrase — no heuristic can regress.
    assert_eq!(
        msg,
        "STORAGE_FAILURE: pipeline stage failed",
        "persisted message must be the generic safe phrase built only from the code"
    );
}
