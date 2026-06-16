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
    #[serde(default)]
    pub leaf_chunk_ids: Vec<ChunkId>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tree_node_deserializes_without_leaf_chunk_ids() {
        // Simulate a JSON row from an older DB schema that has no leaf_chunk_ids field.
        let json = r#"{
            "id": "00000000-0000-0000-0000-000000000001",
            "level": 0,
            "text": "hello",
            "vector": [0.1, 0.2],
            "parent": null,
            "children": [],
            "cluster_centroid": null,
            "source_uri": "file://doc.pdf"
        }"#;
        let node: TreeNode = serde_json::from_str(json).expect("should deserialize with missing leaf_chunk_ids");
        assert!(node.leaf_chunk_ids.is_empty());
    }
}
