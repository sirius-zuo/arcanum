use crate::dag::{PipelineDAG, PipelineStage};
use std::sync::Arc;

/// StandardPipeline: Load → Preprocess → Chunk → Embed → VectorWrite
pub fn build() -> PipelineDAG {
    PipelineDAG::new()
        .add_stage(PipelineStage {
            id: "load",
            deps: vec![],
            run: Arc::new(|ctx| Box::pin(async move {
                tracing::debug!("stage: load");
                Ok(ctx)
            })),
        })
        .add_stage(PipelineStage {
            id: "preprocess",
            deps: vec!["load"],
            run: Arc::new(|ctx| Box::pin(async move {
                tracing::debug!("stage: preprocess");
                Ok(ctx)
            })),
        })
        .add_stage(PipelineStage {
            id: "chunk",
            deps: vec!["preprocess"],
            run: Arc::new(|ctx| Box::pin(async move {
                tracing::debug!("stage: chunk");
                Ok(ctx)
            })),
        })
        .add_stage(PipelineStage {
            id: "embed",
            deps: vec!["chunk"],
            run: Arc::new(|ctx| Box::pin(async move {
                tracing::debug!("stage: embed");
                Ok(ctx)
            })),
        })
        .add_stage(PipelineStage {
            id: "vector_write",
            deps: vec!["embed"],
            run: Arc::new(|ctx| Box::pin(async move {
                tracing::debug!("stage: vector_write");
                Ok(ctx)
            })),
        })
}
