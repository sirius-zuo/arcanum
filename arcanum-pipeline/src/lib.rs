pub mod dag;
pub mod executor;
pub mod templates;

pub use dag::{PipelineDAG, PipelineStage, StageFn, StageContext};
pub use executor::DagExecutor;

pub enum PipelineTemplate {
    Standard,
    Contextual,
    Graph,
    Raptor,
    Full,
    Custom(PipelineDAG),
}
