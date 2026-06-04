use crate::{
    dag::PipelineDAG,
    deps::PipelineDeps,
    ingestion_state::IngestionState,
    registry::TemplateBuilder,
    stages::*,
};
use std::sync::Arc;
use tokio::sync::Mutex;

/// StandardPipeline: Load → Dedup → Cleanup → Preprocess → Chunk → Embed → VectorWrite
pub fn builder() -> TemplateBuilder {
    Arc::new(|state: Arc<Mutex<IngestionState>>, deps: &PipelineDeps| {
        PipelineDAG::new()
            .add_stage(make_load_stage(state.clone(), deps.loaders.clone()))
            .add_stage(make_dedup_stage(state.clone(), deps.document_registry.clone()))
            .add_stage(make_cleanup_stage(
                state.clone(),
                deps.document_registry.clone(),
                deps.vector_store.clone(),
                deps.graph_store.clone(),
                deps.tree_store.clone(),
            ))
            .add_stage(make_preprocess_stage(state.clone(), deps.preprocessors.clone()))
            .add_stage(make_chunk_stage(state.clone(), deps.chunker.clone()))
            .add_stage(make_embed_stage(state.clone(), deps.embedder.clone(), deps.embedding_cb.clone()))
            .add_stage(make_vector_write_stage(state.clone(), deps.vector_store.clone(), deps.vector_store_cb.clone()))
    })
}
