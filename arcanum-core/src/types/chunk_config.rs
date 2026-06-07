use serde::{Deserialize, Serialize};
use std::sync::Arc;
use crate::traits::Chunker;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChunkStrategyConfig {
    pub strategy: String,
    pub params: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    pub experiment_id: ExperimentId,
    pub chunkers:      PerBackendChunkers,
}
