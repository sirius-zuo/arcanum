use crate::{
    deps::PipelineDeps,
    executor::DagExecutor,
    ingestion_state::IngestionState,
    registry::ArcanumPipelineRegistry,
};
use arcanum_core::{
    traits::{ProgressEmitter, Source},
    types::{CollectionId, IngestionTask},
    Result,
};
use arcanum_middleware::BoundedQueue;
use std::sync::Arc;
use tokio::sync::Mutex;

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
    pub async fn process_next(&self) -> Option<Result<()>> {
        let task = self.queue.pop().await?;
        Some(
            run_task(
                &task.source_uri,
                task.collection_id,
                &task.pipeline_template,
                self.registry.clone(),
                self.deps.clone(),
                self.emitter.clone(),
            )
            .await,
        )
    }
}

/// Free function for running a single ingestion task without a full queue.
pub async fn run_task(
    source_uri:        &str,
    collection_id:     CollectionId,
    pipeline_template: &str,
    registry:          Arc<ArcanumPipelineRegistry>,
    deps:              Arc<PipelineDeps>,
    _emitter:          Arc<dyn ProgressEmitter>,
) -> Result<()> {
    let source = Source::from_uri(source_uri)?;
    let state = Arc::new(Mutex::new(IngestionState::new(source, collection_id)));
    let dag = registry.build(pipeline_template, state.clone(), &deps)?;
    let final_ctx = DagExecutor::execute(&dag, Default::default()).await?;

    let skipped = final_ctx.get("__skip").and_then(|v| v.as_bool()).unwrap_or(false);
    if !skipped {
        if let Some(doc) = &state.lock().await.doc {
            deps.hash_tracker.record(&doc.source_uri, &doc.content).await;
        }
    }
    Ok(())
}
