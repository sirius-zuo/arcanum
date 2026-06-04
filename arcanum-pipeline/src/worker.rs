use crate::{
    deps::PipelineDeps,
    executor::DagExecutor,
    ingestion_state::IngestionState,
    registry::ArcanumPipelineRegistry,
};
use arcanum_core::{
    traits::{ProgressEmitter, Source},
    types::{IngestionReport, IngestionStatus, IngestionTask},
    Result,
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
}

impl IngestionWorker {
    pub fn new(
        registry: Arc<ArcanumPipelineRegistry>,
        deps:     Arc<PipelineDeps>,
        emitter:  Arc<dyn ProgressEmitter>,
        queue:    Arc<BoundedQueue<IngestionTask>>,
    ) -> Self {
        Self { registry, deps, emitter, queue }
    }

    /// Pop one task off the queue and run it. Returns `None` when the queue is closed.
    #[instrument(skip(self))]
    pub async fn process_next(&self) -> Option<Result<()>> {
        let task = self.queue.pop().await?;
        Some(
            run_task(
                task,
                self.registry.clone(),
                self.deps.clone(),
                self.emitter.clone(),
                self.queue.clone(),
            )
            .await,
        )
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

    match DagExecutor::execute(&dag, Default::default()).await {
        Ok(final_ctx) => {
            let skipped = final_ctx.get("__skip").and_then(|v| v.as_bool()).unwrap_or(false);
            let state_lock = state.lock().await;

            if !skipped {
                let report = IngestionReport {
                    operation_id:         operation_id.clone(),
                    source_uri:           source_uri.clone(),
                    pipeline_template:    pipeline_template.clone(),
                    stage_results:        vec![],
                    total_chunks:         state_lock.chunks.len(),
                    total_vectors:        state_lock.vectors.len(),
                    document_fingerprint: state_lock.doc.as_ref()
                        .map(|d| d.content_hash())
                        .unwrap_or_default(),
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
