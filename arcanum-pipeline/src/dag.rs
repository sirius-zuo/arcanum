use std::sync::Arc;
use arcanum_core::Result;
use std::collections::HashMap;

pub type StageId = &'static str;
pub type StageContext = HashMap<String, serde_json::Value>;
pub type StageFn = Arc<
    dyn Fn(StageContext) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<StageContext>> + Send>>
    + Send + Sync
>;

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
