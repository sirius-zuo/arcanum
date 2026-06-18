use arcanum_core::{
    Result,
    traits::{IngestionDepsOverrideResolver, Preprocessor},
    types::{PerBackendChunkConfig, PerBackendChunkers, ShadowContext},
};
use arcanum_ingestion::{default_registry, PreprocessorCatalog};
use async_trait::async_trait;
use std::sync::Arc;
use crate::services::{
    collection::CollectionService,
    experiment::{ExperimentService, ExperimentStatus},
};

pub struct EngineIngestionDepsResolver {
    pub collection_service:   Arc<CollectionService>,
    pub experiment_service:   Arc<ExperimentService>,
    pub global_chunking:      PerBackendChunkConfig,
    pub preprocessor_catalog: Arc<PreprocessorCatalog>,
}

#[async_trait]
impl IngestionDepsOverrideResolver for EngineIngestionDepsResolver {
    async fn resolve_for_collection(
        &self,
        collection_id: &str,
    ) -> Result<(PerBackendChunkers, Option<ShadowContext>, Option<Arc<dyn Preprocessor>>)> {
        let col_info = match self.collection_service.get(collection_id).await {
            Ok(info) => info,
            Err(_) => {
                // Collection not found (deleted after task was queued) — use global defaults.
                let chunkers = resolve_chunkers(None, &self.global_chunking)?;
                let preprocessor = self.preprocessor_catalog.get("default");
                return Ok((chunkers, None, preprocessor));
            }
        };

        let chunkers = resolve_chunkers(col_info.chunker_config.as_ref(), &self.global_chunking)?;

        let preprocessor = match &col_info.preprocessor {
            Some(name) => self.preprocessor_catalog.get(name),
            None => self.preprocessor_catalog.get("default"),
        };

        let shadow = if let Some(exp_id) = col_info.experiment {
            match self.experiment_service.get(collection_id, &exp_id).await {
                Ok(exp) if exp.status == ExperimentStatus::Active => {
                    let shadow_col_id = exp.shadow_namespace(collection_id);
                    let shadow_chunkers = resolve_chunkers(
                        Some(&exp.challenger_config),
                        &self.global_chunking,
                    )?;
                    Some(ShadowContext {
                        experiment_id:       exp.id,
                        chunkers:            shadow_chunkers,
                        shadow_collection_id: shadow_col_id,
                    })
                }
                Ok(_) | Err(_) => None,
            }
        } else {
            None
        };

        Ok((chunkers, shadow, preprocessor))
    }
}

fn resolve_chunkers(
    collection_config: Option<&PerBackendChunkConfig>,
    global_config: &PerBackendChunkConfig,
) -> Result<PerBackendChunkers> {
    let registry = default_registry();
    let vector_cfg = collection_config
        .map(|c| &c.vector)
        .unwrap_or(&global_config.vector);
    let graph_cfg = collection_config
        .and_then(|c| c.graph.as_ref())
        .or(global_config.graph.as_ref())
        .unwrap_or(&global_config.vector);
    let tree_cfg = collection_config
        .and_then(|c| c.tree.as_ref())
        .or(global_config.tree.as_ref())
        .unwrap_or(&global_config.vector);
    Ok(PerBackendChunkers {
        vector: registry.build(vector_cfg)?,
        graph:  registry.build(graph_cfg)?,
        tree:   registry.build(tree_cfg)?,
    })
}
