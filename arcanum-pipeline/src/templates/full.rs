use crate::{
    dag::PipelineDAG,
    deps::PipelineDeps,
    ingestion_state::IngestionState,
    registry::TemplateBuilder,
    stages::{self, *},
};
use std::sync::Arc;
use tokio::sync::Mutex;

const DEFAULT_RAPTOR_DEPTH: u32 = 3;

pub fn builder() -> TemplateBuilder {
    Arc::new(|state: Arc<Mutex<IngestionState>>, deps: &PipelineDeps| {
        let mut dag = PipelineDAG::new()
            .add_stage(make_load_stage(state.clone(), deps.loaders.clone()))
            .add_stage(make_dedup_stage(state.clone(), deps.version_store.clone()))
            .add_stage(make_cleanup_stage(
                state.clone(),
                deps.version_store.clone(),
                deps.vector_store.clone(),
                deps.graph_store.clone(),
                deps.tree_store.clone(),
            ))
            .add_stage(make_preprocess_stage(state.clone(), deps.preprocessors.clone()))
            .add_stage(make_snapshot_stage(
                state.clone(),
                deps.version_store.clone(),
                deps.snapshot_store.clone(),
            ))
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
            .add_stage(make_tree_chunk_stage(state.clone(), deps.chunkers.tree.clone()));

        let embed_dep = match &deps.context_enricher {
            Some(e) => {
                dag = dag.add_stage(make_context_enrich_stage(state.clone(), e.clone()));
                "context_enrich"
            }
            None => "vector_chunk",
        };

        dag = dag.add_stage(make_embed_stage_after(embed_dep, state.clone(), deps.embedder.clone(), deps.embedding_cb.clone()));
        dag = dag.add_stage(make_vector_write_stage(state.clone(), deps.vector_store.clone(), deps.vector_store_cb.clone()));

        if let (Some(ext), Some(gs)) = (&deps.entity_extractor, &deps.graph_store) {
            dag = dag.add_stage(make_entity_extract_stage(state.clone(), ext.clone(), gs.clone()));
        }

        if let Some(ts) = &deps.tree_store {
            dag = dag.add_stage(make_tree_embed_stage(
                state.clone(), deps.embedder.clone(), deps.embedding_cb.clone(),
            ));
            dag = dag.add_stage(make_raptor_build_stage(state.clone(), ts.clone(), DEFAULT_RAPTOR_DEPTH));
        }

        dag
    })
}
