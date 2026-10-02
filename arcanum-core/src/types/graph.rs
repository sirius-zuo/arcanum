use super::document::ChunkId;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityId(pub Uuid);

impl Default for EntityId {
    fn default() -> Self {
        Self::new()
    }
}

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
    #[serde(default)]
    pub collection_id: String,
}

/// An entity returned by `GraphStore::query`, with its hop distance from the
/// nearest seed (name/type match). Seeds have `hops == 0`.
#[derive(Debug, Clone)]
pub struct EntityHit {
    pub entity: Entity,
    pub hops: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Relation {
    pub source: EntityId,
    pub relation_type: String,
    pub target: EntityId,
    pub confidence: f32,
    pub source_chunks: Vec<ChunkId>,
}
