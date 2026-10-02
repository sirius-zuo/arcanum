use crate::services::{
    collection::CollectionService,
    experiment::{ExperimentService, ExperimentStatus},
};
use arcanum_core::{
    traits::{IngestionDepsOverrideResolver, Preprocessor},
    types::{PerBackendChunkConfig, PerBackendChunkers, ShadowContext},
    Result,
};
use arcanum_ingestion::{default_registry, PreprocessorCatalog};
use async_trait::async_trait;
use std::sync::Arc;

pub struct EngineIngestionDepsResolver {
    pub collection_service: Arc<CollectionService>,
    pub experiment_service: Arc<ExperimentService>,
    pub global_chunking: PerBackendChunkConfig,
    pub preprocessor_catalog: Arc<PreprocessorCatalog>,
}

#[async_trait]
impl IngestionDepsOverrideResolver for EngineIngestionDepsResolver {
    async fn resolve_for_collection(
        &self,
        collection_id: &str,
    ) -> Result<(
        PerBackendChunkers,
        Option<ShadowContext>,
        Option<Arc<dyn Preprocessor>>,
    )> {
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
                    let shadow_chunkers =
                        resolve_chunkers(Some(&exp.challenger_config), &self.global_chunking)?;
                    Some(ShadowContext {
                        experiment_id: exp.id,
                        chunkers: shadow_chunkers,
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

pub(crate) fn resolve_chunkers(
    collection_config: Option<&PerBackendChunkConfig>,
    global_config: &PerBackendChunkConfig,
) -> Result<PerBackendChunkers> {
    let registry = default_registry();
    let vector_cfg = collection_config
        .map(|c| &c.vector)
        .unwrap_or(&global_config.vector);
    let lexical_cfg = collection_config
        .and_then(|c| c.lexical.as_ref())
        .or(global_config.lexical.as_ref())
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
        lexical: registry.build(lexical_cfg)?,
        graph: registry.build(graph_cfg)?,
        tree: registry.build(tree_cfg)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use arcanum_core::types::{ChunkStrategyConfig, RawDocument};

    fn fixed(size: u64) -> ChunkStrategyConfig {
        ChunkStrategyConfig {
            strategy: "fixed".to_string(),
            params: serde_json::json!({ "chunk_size": size, "overlap": 0 }),
        }
    }

    fn doc() -> RawDocument {
        RawDocument::for_test("word ".repeat(100).into_bytes(), "text/plain")
    }

    #[tokio::test]
    async fn resolve_chunkers_lexical_falls_back_to_vector_config() {
        let global = PerBackendChunkConfig {
            vector: fixed(100),
            ..Default::default()
        };
        let c = resolve_chunkers(None, &global).unwrap();
        assert!(!Arc::ptr_eq(&c.lexical, &c.vector));
        let d = doc();
        let lexical = c.lexical.chunk(&d).await.unwrap();
        let vector = c.vector.chunk(&d).await.unwrap();
        assert_eq!(lexical.len(), vector.len());
    }

    #[tokio::test]
    async fn resolve_chunkers_prefers_collection_lexical() {
        let global = PerBackendChunkConfig {
            vector: fixed(200),
            lexical: Some(fixed(150)),
            ..Default::default()
        };
        let collection = PerBackendChunkConfig {
            vector: fixed(200),
            lexical: Some(fixed(50)),
            ..Default::default()
        };
        let c = resolve_chunkers(Some(&collection), &global).unwrap();
        let d = doc();
        let lexical = c.lexical.chunk(&d).await.unwrap();
        let vector = c.vector.chunk(&d).await.unwrap();
        assert!(lexical.len() > vector.len());
        let g = resolve_chunkers(None, &global).unwrap();
        let global_lexical = g.lexical.chunk(&d).await.unwrap();
        assert!(lexical.len() > global_lexical.len());
    }
}
