use serde::{Deserialize, Serialize};
use uuid::Uuid;
use super::document::{ChunkId, Vector};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TreeNodeId(pub Uuid);

impl TreeNodeId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TreeNode {
    pub id: TreeNodeId,
    pub level: u32,
    pub text: String,
    pub vector: Vector,
    pub parent: Option<TreeNodeId>,
    pub children: Vec<TreeNodeId>,
    pub cluster_centroid: Option<Vector>,
    #[serde(default)]
    pub source_uri: String,
    pub leaf_chunk_ids: Vec<ChunkId>,
}
