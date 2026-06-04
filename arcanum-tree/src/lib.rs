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

    async fn delete_by_source_uri(&self, collection: &str, source_uri: &str) -> Result<()> {
        let prefix = format!("{}:", collection);
        let mut nodes = self.nodes.write().await;
        for (key, level_nodes) in nodes.iter_mut() {
            if key.starts_with(&prefix) {
                level_nodes.retain(|n| n.source_uri != source_uri);
            }
        }
        Ok(())
    }
}

pub use raptor::RaptorBuilder;
mod raptor;

pub mod postgres_store;
pub use postgres_store::PgTreeStore;

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn delete_by_source_uri_removes_nodes() {
        let store = InMemoryTreeStore::new();
        store.insert_node("col", TreeNode {
            id: TreeNodeId::new(), level: 0, text: "chunk a".into(),
            vector: Vector(vec![0.1]), parent: None, children: vec![],
            cluster_centroid: None, source_uri: "file://a.md".into(),
        }).await.unwrap();
        store.insert_node("col", TreeNode {
            id: TreeNodeId::new(), level: 0, text: "chunk b".into(),
            vector: Vector(vec![0.2]), parent: None, children: vec![],
            cluster_centroid: None, source_uri: "file://b.md".into(),
        }).await.unwrap();
        store.delete_by_source_uri("col", "file://a.md").await.unwrap();
        let nodes = store.get_level("col", 0).await.unwrap();
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].source_uri, "file://b.md");
    }
}
