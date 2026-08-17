use crate::{
    deps::PipelineDeps,
    dag::{CTX_FORCE, CTX_SKIP},
    executor::DagExecutor,
    ingestion_state::IngestionState,
    registry::ArcanumPipelineRegistry,
};
use arcanum_core::{
    traits::{OperationStore, ProgressEmitter, Source},
    types::{IngestionProgressReport, IngestionReport, IngestionStatus, IngestionTask},
    ArcanumError, Result,
};
use arcanum_middleware::BoundedQueue;
use chrono::Utc;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::instrument;

pub struct IngestionWorker {
    registry: Arc<ArcanumPipelineRegistry>,
    deps:     Arc<PipelineDeps>,
    emitter:  Arc<dyn ProgressEmitter>,
    queue:    Arc<BoundedQueue<IngestionTask>>,
    operations: Arc<dyn OperationStore>,
    resolver: Option<Arc<dyn arcanum_core::traits::IngestionDepsOverrideResolver>>,
}

impl IngestionWorker {
    pub fn new(
        registry: Arc<ArcanumPipelineRegistry>,
        deps:     Arc<PipelineDeps>,
        emitter:  Arc<dyn ProgressEmitter>,
        queue:    Arc<BoundedQueue<IngestionTask>>,
        operations: Arc<dyn OperationStore>,
    ) -> Self {
        Self { registry, deps, emitter, queue, operations, resolver: None }
    }

    /// Attach a per-job resolver. Workers without a resolver use the shared base deps.
    pub fn with_resolver(
        mut self,
        resolver: Arc<dyn arcanum_core::traits::IngestionDepsOverrideResolver>,
    ) -> Self {
        self.resolver = Some(resolver);
        self
    }

    /// Pop one task off the queue and run it. Returns `None` when the queue is closed.
    #[instrument(skip(self))]
    pub async fn process_next(&self) -> Option<Result<()>> {
        let task = self.queue.pop().await?;
        let deps = self.resolve_task_deps(&task.collection_id.0).await;
        Some(
            run_task(
                task,
                self.registry.clone(),
                deps,
                self.emitter.clone(),
                self.operations.clone(),
            )
            .await,
        )
    }

    async fn resolve_task_deps(&self, collection_id: &str) -> Arc<PipelineDeps> {
        let Some(resolver) = &self.resolver else { return self.deps.clone(); };
        match resolver.resolve_for_collection(collection_id).await {
            Ok((chunkers, shadow, preprocessors)) => {
                Arc::new(PipelineDeps {
                    chunkers,
                    shadow,
                    preprocessors,
                    // All other fields are cheap Arc clones from the shared base deps.
                    loaders:           self.deps.loaders.clone(),
                    context_enricher:  self.deps.context_enricher.clone(),
                    entity_extractor:  self.deps.entity_extractor.clone(),
                    embedder:          self.deps.embedder.clone(),
                    vector_store:      self.deps.vector_store.clone(),
                    graph_store:       self.deps.graph_store.clone(),
                    tree_store:        self.deps.tree_store.clone(),
                    version_store:     self.deps.version_store.clone(),
                    snapshot_store:    self.deps.snapshot_store.clone(),
                    chunk_metadata:    self.deps.chunk_metadata.clone(),
                    bm25_index:        self.deps.bm25_index.clone(),
                    retry_policy:      self.deps.retry_policy.clone(),
                    cache_invalidator: self.deps.cache_invalidator.clone(),
                    embedding_cb:      self.deps.embedding_cb.clone(),
                    vector_store_cb:   self.deps.vector_store_cb.clone(),
                })
            }
            Err(e) => {
                tracing::warn!(
                    collection_id = %collection_id,
                    err = ?e,
                    "per-job deps resolution failed — falling back to global defaults"
                );
                self.deps.clone()
            }
        }
    }
}

/// Classify an internal pipeline error into a stable, safe error code and a
/// retryability hint for the durable `SafeOperationError`. Codes are redacted —
/// no internal stack or connection details — and only transient infrastructure
/// failures (embedding, enrichment, ingestion, pipeline, full queue) are
/// retryable.
fn classify_error(err: &ArcanumError) -> (&'static str, bool) {
    match err {
        ArcanumError::Storage(_) => ("STORAGE_FAILURE", false),
        ArcanumError::Embedding(_) => ("EMBEDDING_FAILURE", true),
        ArcanumError::Enrichment(_) => ("ENRICHMENT_FAILURE", true),
        ArcanumError::Ingestion(_) => ("INGESTION_FAILURE", true),
        ArcanumError::Retrieval(_) => ("RETRIEVAL_FAILURE", true),
        ArcanumError::Config(_) => ("CONFIG_ERROR", false),
        ArcanumError::Auth(_) => ("AUTH_ERROR", false),
        ArcanumError::NotFound(_) => ("NOT_FOUND", false),
        ArcanumError::AlreadyExists(_) => ("ALREADY_EXISTS", false),
        ArcanumError::Conflict(_) => ("CONFLICT", false),
        ArcanumError::QueueFull => ("QUEUE_FULL", true),
        ArcanumError::Pipeline { .. } => ("PIPELINE_FAILURE", true),
        ArcanumError::Other(_) => ("INTERNAL_ERROR", false),
    }
}

/// Produce a fully generic safe message for a durable `SafeOperationError`.
/// The message is built ONLY from the stable error code — the underlying
/// error text is never reproduced, so no connection URL, hostname, filesystem
/// path, or bare `host:port` token can leak into the persisted report (Task 4
/// serves it verbatim over the query API). The code plus the `retryable` flag
/// already convey the failure class and recoverability.
fn sanitize_error_message(code: &str) -> String {
    format!("{code}: pipeline stage failed")
}

/// Free function for running a single ingestion task without a full queue.
///
/// Persists every transition to the `OperationStore`: `Accepted -> Running`
/// before preprocessing, then a terminal `Succeeded` (with the original-content
/// URI, or the existing version's snapshot URI for unchanged content) or
/// `Failed` (with a safe, redacted error) BEFORE any event is emitted. A
/// terminal `Failed` operation is final — the worker never re-enqueues a retry
/// for it.
#[instrument(skip(task, registry, deps, emitter, operations), fields(source_uri = %task.source_uri), err)]
pub async fn run_task(
    task:      IngestionTask,
    registry:  Arc<ArcanumPipelineRegistry>,
    deps:      Arc<PipelineDeps>,
    emitter:   Arc<dyn ProgressEmitter>,
    operations: Arc<dyn OperationStore>,
) -> Result<()> {
    let operation_id       = task.operation_id.clone();
    let source_uri         = task.source_uri.clone();
    let collection_id      = task.collection_id.clone();
    let pipeline_template  = task.pipeline_template.clone();
    let force              = task.force;

    // Persist Accepted -> Running BEFORE doing any work so a restart can see
    // that the operation was picked up.
    if let Err(err) = operations.mark_running(&operation_id, Utc::now()).await {
        metrics::counter!("arcanum_ingest_docs_total",
            "source" => source_uri.clone(), "status" => "error").increment(1);
        return Err(err);
    }

    // Run the pipeline and build the durable terminal report. Any failure —
    // including a missing snapshot/version needed to build a Succeeded report —
    // becomes a Failed report so the store is always consistent.
    let outcome = async {
        let source = match &task.content {
            Some(bytes) => Source::Raw {
                content: bytes.clone(),
                mime_hint: task.mime_hint.clone(),
                uri: source_uri.clone(),
            },
            None => Source::from_uri(&source_uri)?,
        };
        let state  = Arc::new(Mutex::new(IngestionState::new(source, collection_id.clone())));
        let dag    = registry.build(&pipeline_template, state.clone(), &deps)?;

        let mut initial_ctx = crate::dag::StageContext::default();
        initial_ctx.insert(CTX_FORCE.to_string(), serde_json::json!(force));
        let final_ctx = DagExecutor::execute(&dag, initial_ctx).await?;

        let skipped = final_ctx.get(CTX_SKIP).and_then(|v| v.as_bool()).unwrap_or(false);

        let report = {
            let state_lock = state.lock().await;
            if skipped {
                // Deduplication reported unchanged content: return the EXISTING
                // collection-scoped version's snapshot URI, never a fresh one.
                let latest = deps.version_store.get_latest(&source_uri, &collection_id.0).await?
                    .ok_or_else(|| ArcanumError::Pipeline {
                        stage: "worker".into(),
                        message: "dedup reported unchanged content but no prior version exists".into(),
                    })?;
                IngestionReport::unchanged(operation_id.clone(), latest.snapshot_uri.clone())
            } else {
                // New or changed content: the snapshot stage produced the durable
                // original-content URI.
                let snapshot_uri = state_lock.snapshot_uri.clone().ok_or_else(|| ArcanumError::Pipeline {
                    stage: "worker".into(),
                    message: "pipeline succeeded but snapshot_uri is None — cannot compute content URI".into(),
                })?;
                IngestionReport::succeeded(operation_id.clone(), snapshot_uri)
            }
        }; // state_lock dropped before `state` is returned

        Ok::<_, ArcanumError>((final_ctx, skipped, report, state))
    }.await;

    match outcome {
        Ok((final_ctx, skipped, report, state)) => {
            // Build the live-progress payload BEFORE persisting the terminal
            // report. Every fallible computation happens here so `complete` is
            // the last operation that can fail and the durable state can never
            // disagree with the returned result. A progress-report build failure
            // is logged and skipped (the durable Succeeded report is the truth).
            let progress: Option<IngestionProgressReport> = if skipped {
                None
            } else {
                let built = async {
                    let state_lock = state.lock().await;
                    let doc = state_lock.doc.as_ref().ok_or_else(|| ArcanumError::Pipeline {
                        stage: "worker".into(),
                        message: "pipeline succeeded but doc is None — cannot compute fingerprint".into(),
                    })?;
                    let content_hash = doc.content_hash();
                    let failed_stages: Vec<String> = final_ctx
                        .get(crate::dag::CTX_STAGE_FAILURES)
                        .and_then(|v| v.as_array())
                        .map(|arr| arr.iter()
                            .filter_map(|f| f["stage"].as_str().map(String::from))
                            .collect())
                        .unwrap_or_default();
                    let status = if failed_stages.is_empty() {
                        IngestionStatus::Success
                    } else {
                        IngestionStatus::PartialSuccess { failed_stages }
                    };
                    Ok::<_, ArcanumError>(IngestionProgressReport {
                        operation_id:         operation_id.clone(),
                        source_uri:           source_uri.clone(),
                        pipeline_template:    pipeline_template.clone(),
                        stage_results:        vec![],
                        total_chunks:         state_lock.chunks.len(),
                        total_vectors:        state_lock.vectors.len(),
                        document_fingerprint: content_hash,
                        status,
                    })
                }
                .await;
                match built {
                    Ok(p) => Some(p),
                    Err(err) => {
                        tracing::warn!(
                            op_id = %operation_id.0,
                            err = ?err,
                            "pipeline succeeded but the live progress report could not be built"
                        );
                        None
                    }
                }
            };

            // Persist the terminal report BEFORE emitting any live event.
            if let Err(err) = operations.complete(&report).await {
                metrics::counter!("arcanum_ingest_docs_total",
                    "source" => source_uri.clone(), "status" => "error").increment(1);
                return Err(err);
            }

            if skipped {
                emitter.emit("ingestion:progress", serde_json::json!({
                    "operation_id": operation_id.0,
                    "status": "skipped",
                    "reason": "content_unchanged",
                })).await;
                return Ok(());
            }

            // Covers force (dedup always sets __replace, never __skip,
            // for a forced task), a genuine content change (dedup's
            // hash mismatch), and a brand-new document (harmless no-op
            // — nothing was cached under this source_uri yet).
            deps.cache_invalidator
                .invalidate_document(&source_uri, &collection_id)
                .await;
            if let Some(progress) = progress {
                emitter.emit("ingestion:progress", serde_json::json!({
                    "operation_id": operation_id.0,
                    "status": "completed",
                    "report": serde_json::to_value(&progress).unwrap_or_default(),
                })).await;
            }
            Ok(())
        }
        Err(e) => {
            metrics::counter!("arcanum_ingest_docs_total",
                "source" => source_uri.clone(), "status" => "error").increment(1);

            // Persist the terminal failure BEFORE surfacing the error; the store
            // is authoritative. `Failed` is terminal — the worker never
            // re-enqueues a retry for it, so the operation is not reprocessed.
            // The persist is best-effort so a store hiccup does not mask the
            // original pipeline error.
            let (code, retryable) = classify_error(&e);
            let failed = IngestionReport::failed(
                operation_id.clone(),
                code,
                sanitize_error_message(code),
                retryable,
            );
            if let Err(complete_err) = operations.complete(&failed).await {
                tracing::warn!(
                    op_id = %operation_id.0,
                    err = %complete_err,
                    "failed to persist terminal failure for operation"
                );
            }
            Err(e)
        }
    }
}
