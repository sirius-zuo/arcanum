use serde::{Deserialize, Serialize};
use std::sync::Arc;
use crate::traits::Chunker;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChunkStrategyConfig {
    pub strategy: String,
    pub params: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PerBackendChunkConfig {
    pub vector: ChunkStrategyConfig,
    pub graph:  Option<ChunkStrategyConfig>,
    pub tree:   Option<ChunkStrategyConfig>,
}

impl Default for PerBackendChunkConfig {
    fn default() -> Self {
        Self {
            vector: ChunkStrategyConfig {
                strategy: "fixed".to_string(),
                params: serde_json::json!({ "chunk_size": 512, "overlap": 64 }),
            },
            graph: None,
            tree:  None,
        }
    }
}

pub struct PerBackendChunkers {
    pub vector: Arc<dyn Chunker>,
    pub graph:  Arc<dyn Chunker>,
    pub tree:   Arc<dyn Chunker>,
}

impl Clone for PerBackendChunkers {
    fn clone(&self) -> Self {
        Self {
            vector: self.vector.clone(),
            graph:  self.graph.clone(),
            tree:   self.tree.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ExperimentId(pub uuid::Uuid);

impl ExperimentId {
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4())
    }
}

impl Default for ExperimentId {
    fn default() -> Self { Self::new() }
}

pub struct ShadowContext {
    pub experiment_id:       ExperimentId,
    pub chunkers:            PerBackendChunkers,
    /// Pre-computed shadow namespace: "{{collection_id}}__shadow_{{experiment_id}}".
    /// Computed once by the resolver so stages never need the collection_id at hand.
    pub shadow_collection_id: String,
}

impl Clone for ShadowContext {
    fn clone(&self) -> Self {
        Self {
            experiment_id:       self.experiment_id.clone(),
            chunkers:            self.chunkers.clone(),
            shadow_collection_id: self.shadow_collection_id.clone(),
        }
    }
}
