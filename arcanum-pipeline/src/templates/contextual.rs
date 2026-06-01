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

pub fn builder() -> TemplateBuilder {
    Arc::new(|state: Arc<Mutex<IngestionState>>, deps: &PipelineDeps| {
        match &deps.context_enricher {
            None => {
                tracing::warn!("context_enricher not configured — falling back to Standard pipeline");
                standard::builder()(state, deps)
            }
            Some(enricher) => {
                PipelineDAG::new()
                    .add_stage(make_load_stage(state.clone(), deps.loaders.clone(), deps.hash_tracker.clone()))
                    .add_stage(make_preprocess_stage(state.clone(), deps.preprocessors.clone()))
                    .add_stage(make_chunk_stage(state.clone(), deps.chunker.clone()))
                    .add_stage(make_context_enrich_stage(state.clone(), enricher.clone()))
                    .add_stage(make_embed_stage_after("context_enrich", state.clone(), deps.embedder.clone(), deps.embedding_cb.clone()))
                    .add_stage(make_vector_write_stage(state.clone(), deps.vector_store.clone(), deps.vector_store_cb.clone()))
            }
        }
    })
}
