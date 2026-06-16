use crate::{
    deps::PipelineDeps,
    dag::{CTX_FORCE, CTX_SKIP},
    executor::DagExecutor,
    ingestion_state::IngestionState,
    registry::ArcanumPipelineRegistry,
};
use arcanum_core::{
    traits::{ProgressEmitter, Source},
    types::{IngestionReport, IngestionStatus, IngestionTask},
    ArcanumError, Result,
};
use arcanum_middleware::BoundedQueue;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::instrument;

pub struct IngestionWorker {
    registry: Arc<ArcanumPipelineRegistry>,
    deps:     Arc<PipelineDeps>,
    emitter:  Arc<dyn ProgressEmitter>,
    queue:    Arc<BoundedQueue<IngestionTask>>,
    resolver: Option<Arc<dyn arcanum_core::traits::IngestionDepsOverrideResolver>>,
}

impl IngestionWorker {
    pub fn new(
        registry: Arc<ArcanumPipelineRegistry>,
        deps:     Arc<PipelineDeps>,
        emitter:  Arc<dyn ProgressEmitter>,
        queue:    Arc<BoundedQueue<IngestionTask>>,
    ) -> Self {
        Self { registry, deps, emitter, queue, resolver: None }
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
                self.queue.clone(),
            )
            .await,
        )
    }

    async fn resolve_task_deps(&self, collection_id: &str) -> Arc<PipelineDeps> {
        let Some(resolver) = &self.resolver else { return self.deps.clone(); };
        match resolver.resolve_for_collection(collection_id).await {
            Ok((chunkers, shadow)) => {
                Arc::new(PipelineDeps {
                    chunkers,
                    shadow,
                    // All other fields are cheap Arc clones from the shared base deps.
                    loaders:           self.deps.loaders.clone(),
                    preprocessors:     self.deps.preprocessors.clone(),
                    context_enricher:  self.deps.context_enricher.clone(),
                    entity_extractor:  self.deps.entity_extractor.clone(),
                    embedder:          self.deps.embedder.clone(),
                    vector_store:      self.deps.vector_store.clone(),
                    graph_store:       self.deps.graph_store.clone(),
                    tree_store:        self.deps.tree_store.clone(),
                    version_store:     self.deps.version_store.clone(),
                    snapshot_store:    self.deps.snapshot_store.clone(),
                    chunk_metadata:    self.deps.chunk_metadata.clone(),
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

/// Free function for running a single ingestion task without a full queue.
#[instrument(skip(task, registry, deps, emitter, queue), fields(source_uri = %task.source_uri), err)]
pub async fn run_task(
    task:      IngestionTask,
    registry:  Arc<ArcanumPipelineRegistry>,
    deps:      Arc<PipelineDeps>,
    emitter:   Arc<dyn ProgressEmitter>,
    queue:     Arc<BoundedQueue<IngestionTask>>,
) -> Result<()> {
    let task_attempt       = task.attempt;
    let operation_id       = task.operation_id.clone();
    let source_uri         = task.source_uri.clone();
    let collection_id      = task.collection_id.clone();
    let pipeline_template  = task.pipeline_template.clone();
    let force              = task.force;

    if force {
        deps.cache_invalidator
            .invalidate_document(&source_uri, &collection_id)
            .await;
    }

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
    match DagExecutor::execute(&dag, initial_ctx).await {
        Ok(final_ctx) => {
            let skipped = final_ctx.get(CTX_SKIP).and_then(|v| v.as_bool()).unwrap_or(false);
            let state_lock = state.lock().await;

            if skipped {
                emitter.emit("ingestion:progress", serde_json::json!({
                    "operation_id": operation_id.0,
                    "status": "skipped",
                    "reason": "content_unchanged",
                })).await;
            } else {
                let doc = state_lock.doc.as_ref().ok_or_else(|| ArcanumError::Pipeline {
                    stage: "worker".into(),
                    message: "pipeline succeeded but doc is None — cannot compute fingerprint".into(),
                })?;
                let content_hash = doc.content_hash();
                let report = IngestionReport {
                    operation_id:         operation_id.clone(),
                    source_uri:           source_uri.clone(),
                    pipeline_template:    pipeline_template.clone(),
                    stage_results:        vec![],
                    total_chunks:         state_lock.chunks.len(),
                    total_vectors:        state_lock.vectors.len(),
                    document_fingerprint: content_hash,
                    status:               IngestionStatus::Success,
                };
                emitter.emit("ingestion:progress", serde_json::json!({
                    "operation_id": operation_id.0,
                    "status": "completed",
                    "report": serde_json::to_value(&report).unwrap_or_default(),
                })).await;
            }
            Ok(())
        }
        Err(e) => {
            metrics::counter!("arcanum_ingest_docs_total",
                "source" => source_uri.clone(), "status" => "error").increment(1);
            if deps.retry_policy.should_retry(task_attempt) {
                tokio::time::sleep(deps.retry_policy.delay_for_attempt(task_attempt)).await;
                let retry_task = IngestionTask {
                    operation_id:      operation_id.clone(),
                    source_uri:        source_uri.clone(),
                    collection_id:     collection_id.clone(),
                    pipeline_template: pipeline_template.clone(),
                    attempt:           task_attempt + 1,
                    force:             force,
                    content:           task.content.clone(),
                    mime_hint:         task.mime_hint.clone(),
                };
                let _ = queue.push(retry_task).await;
            }
            Err(e)
        }
    }
}
