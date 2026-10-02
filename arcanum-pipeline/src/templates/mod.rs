pub mod contextual;
pub mod full;
pub mod graph;
pub mod raptor;
pub mod standard;

use crate::{
    dag::PipelineDAG,
    deps::PipelineDeps,
    ingestion_state::IngestionState,
    stages::{make_lexical_chunk_stage, make_lexical_write_stage},
};
use std::sync::Arc;
use tokio::sync::Mutex;

/// Adds the independent lexical ingestion line (`lexical_chunk` then `lexical_write`) iff a
/// BM25 index is configured.
pub(crate) fn with_lexical_stages(
    dag: PipelineDAG,
    state: Arc<Mutex<IngestionState>>,
    deps: &PipelineDeps,
) -> PipelineDAG {
    match &deps.bm25_index {
        Some(index) => dag
            .add_stage(make_lexical_chunk_stage(
                state.clone(),
                deps.chunkers.lexical.clone(),
            ))
            .add_stage(make_lexical_write_stage(
                state,
                index.clone(),
                deps.chunk_metadata.clone(),
            )),
        None => dag,
    }
}
