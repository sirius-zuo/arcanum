use std::sync::Arc;
use arcanum_core::Result;
use std::collections::HashMap;

pub type StageId = &'static str;
pub type StageContext = HashMap<String, serde_json::Value>;
pub type StageFn = Arc<
    dyn Fn(StageContext) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<StageContext>> + Send>>
    + Send + Sync
>;

/// Context flag set by the worker to force cleanup+re-ingest regardless of hash.
pub const CTX_FORCE:   &str = "__force";
/// Context flag set by the dedup stage when content is unchanged — pipeline stages skip.
pub const CTX_SKIP:    &str = "__skip";
/// Context flag set by the dedup stage when content changed or recovery is needed.
pub const CTX_REPLACE: &str = "__replace";


pub struct PipelineStage {
    pub id: StageId,
    pub deps: Vec<StageId>,
    pub run: StageFn,
}

pub struct PipelineDAG {
    pub stages: Vec<PipelineStage>,
}

impl PipelineDAG {
    pub fn new() -> Self { Self { stages: vec![] } }

    pub fn add_stage(mut self, stage: PipelineStage) -> Self {
        self.stages.push(stage);
        self
    }
}
