use serde::{Deserialize, Serialize};
use uuid::Uuid;
use super::document::ChunkId;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityId(pub Uuid);

impl EntityId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entity {
    pub id: EntityId,
    pub name: String,
    pub entity_type: String,
    pub canonical_id: Option<String>,
    pub source_chunks: Vec<ChunkId>,
    #[serde(default)]
    pub source_uri: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Relation {
    pub source: EntityId,
    pub relation_type: String,
    pub target: EntityId,
    pub confidence: f32,
    pub source_chunk: ChunkId,
}
