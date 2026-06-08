use crate::{
    dag::PipelineDAG,
    deps::PipelineDeps,
    ingestion_state::IngestionState,
    registry::TemplateBuilder,
    stages::{self, *},
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
                    .add_stage(make_vector_chunk_stage(
                        state.clone(),
                        deps.chunkers.vector.clone(),
                        deps.shadow.as_ref().map(|s| stages::ShadowWriteContext {
                            chunker:              s.chunkers.vector.clone(),
                            shadow_collection_id: s.shadow_collection_id.clone(),
                            embedder:             deps.embedder.clone(),
                            vector_store:         deps.vector_store.clone(),
                            vector_store_cb:      deps.vector_store_cb.clone(),
                        }),
                    ))
                    .add_stage(make_graph_chunk_stage(state.clone(), deps.chunkers.graph.clone()))
                    .add_stage(make_tree_chunk_stage(state.clone(), deps.chunkers.tree.clone()))
                    .add_stage(make_context_enrich_stage(state.clone(), enricher.clone()))
                    .add_stage(make_embed_stage_after("context_enrich", state.clone(), deps.embedder.clone(), deps.embedding_cb.clone()))
                    .add_stage(make_vector_write_stage(state.clone(), deps.vector_store.clone(), deps.vector_store_cb.clone()))
            }
        }
    })
}
