pub mod dag;
pub mod executor;
pub mod templates;
pub mod ingestion_state;
pub mod deps;
pub mod stage_failure;
pub mod stages;

pub use dag::{PipelineDAG, PipelineStage, StageFn, StageContext};
pub use executor::DagExecutor;
pub use ingestion_state::IngestionState;
pub use deps::PipelineDeps;
pub use stage_failure::{StageFailure, is_core_stage};

pub enum PipelineTemplate {
    Standard,
    Contextual,
    Graph,
    Raptor,
    Full,
    Custom(PipelineDAG),
}
