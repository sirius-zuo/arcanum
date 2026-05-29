use arcanum_core::{traits::TreeStore, types::*, Result};
use async_trait::async_trait;
use std::{collections::HashMap, sync::Arc};
use tokio::sync::RwLock;

pub struct InMemoryTreeStore {
    nodes: Arc<RwLock<HashMap<String, Vec<TreeNode>>>>,
}

impl InMemoryTreeStore {
    pub fn new() -> Self {
        Self { nodes: Arc::new(RwLock::new(HashMap::new())) }
    }
}

#[async_trait]
impl TreeStore for InMemoryTreeStore {
    async fn insert_node(&self, collection: &str, node: TreeNode) -> Result<()> {
        let key = format!("{}:{}", collection, node.level);
        self.nodes.write().await.entry(key).or_default().push(node);
        Ok(())
    }

    async fn get_level(&self, collection: &str, level: u32) -> Result<Vec<TreeNode>> {
        let key = format!("{}:{}", collection, level);
        Ok(self.nodes.read().await.get(&key).cloned().unwrap_or_default())
    }

    async fn get_children(&self, node_id: &TreeNodeId) -> Result<Vec<TreeNode>> {
        let nodes = self.nodes.read().await;
        Ok(nodes.values().flatten()
            .filter(|n| n.parent.as_ref().map(|p| p.0 == node_id.0).unwrap_or(false))
            .cloned()
            .collect())
    }
}

pub use raptor::RaptorBuilder;
mod raptor;
