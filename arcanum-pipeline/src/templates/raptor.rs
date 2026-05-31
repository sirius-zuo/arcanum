use crate::{
    dag::PipelineDAG,
    deps::PipelineDeps,
    ingestion_state::IngestionState,
    registry::TemplateBuilder,
    stages::*,
    templates::standard,
};
use std::sync::Arc;
use tokio::sync::Mutex;

const DEFAULT_RAPTOR_DEPTH: u32 = 3;

pub fn builder() -> TemplateBuilder {
    Arc::new(|state: Arc<Mutex<IngestionState>>, deps: &PipelineDeps| {
        match &deps.tree_store {
            Some(tree_store) => {
                PipelineDAG::new()
                    .add_stage(make_load_stage(state.clone(), deps.loaders.clone(), deps.hash_tracker.clone()))
                    .add_stage(make_preprocess_stage(state.clone(), deps.preprocessors.clone()))
                    .add_stage(make_chunk_stage(state.clone(), deps.chunker.clone()))
                    .add_stage(make_embed_stage(state.clone(), deps.embedder.clone(), deps.embedding_cb.clone()))
                    .add_stage(make_vector_write_stage(state.clone(), deps.vector_store.clone(), deps.vector_store_cb.clone()))
                    .add_stage(make_raptor_build_stage(state.clone(), tree_store.clone(), DEFAULT_RAPTOR_DEPTH))
            }
            None => {
                tracing::warn!("tree_store not configured — falling back to Standard");
                standard::builder()(state, deps)
            }
        }
    })
}
