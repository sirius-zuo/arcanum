use arcanum_core::{traits::TreeStore, types::*, Result};
use async_trait::async_trait;
use std::{collections::HashMap, sync::Arc};
use tokio::sync::RwLock;
use tracing::instrument;

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
    #[instrument(skip(self, node), fields(store = "in_memory_tree", collection, node_id = %node.id.0, level = node.level), err)]
    async fn insert_node(&self, collection: &str, node: TreeNode) -> Result<()> {
        let key = format!("{}:{}", collection, node.level);
        self.nodes.write().await.entry(key).or_default().push(node);
        Ok(())
    }

    #[instrument(skip(self), fields(store = "in_memory_tree", collection, level, node_count), err)]
    async fn get_level(&self, collection: &str, level: u32) -> Result<Vec<TreeNode>> {
        let key = format!("{}:{}", collection, level);
        let nodes = self.nodes.read().await.get(&key).cloned().unwrap_or_default();
        tracing::Span::current().record("node_count", nodes.len());
        Ok(nodes)
    }

    #[instrument(skip(self, node_id), fields(store = "in_memory_tree", node_id = %node_id.0, child_count), err)]
    async fn get_children(&self, node_id: &TreeNodeId) -> Result<Vec<TreeNode>> {
        let nodes = self.nodes.read().await;
        let children: Vec<TreeNode> = nodes.values().flatten()
            .filter(|n| n.parent.as_ref().map(|p| p.0 == node_id.0).unwrap_or(false))
            .cloned()
            .collect();
        tracing::Span::current().record("child_count", children.len());
        Ok(children)
    }
}

pub use raptor::RaptorBuilder;
mod raptor;

pub mod postgres_store;
pub use postgres_store::PgTreeStore;
