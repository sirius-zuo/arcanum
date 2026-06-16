use arcanum_core::{traits::TreeStore, types::*, ArcanumError, Result};
use async_trait::async_trait;
use std::{collections::HashMap, sync::Arc};
use tokio::sync::RwLock;
use tracing::instrument;

pub struct InMemoryTreeStore {
    nodes: Arc<RwLock<HashMap<String, Vec<TreeNode>>>>,
    created: Arc<RwLock<std::collections::HashSet<String>>>,
}

impl InMemoryTreeStore {
    pub fn new() -> Self {
        Self {
            nodes: Arc::new(RwLock::new(HashMap::new())),
            created: Arc::new(RwLock::new(std::collections::HashSet::new())),
        }
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
        if source_uri.is_empty() {
            tracing::warn!(store = "in_memory_tree", "delete_by_source_uri called with empty source_uri — skipping");
            return Ok(());
        }
        let prefix = format!("{}:", collection);
        let mut nodes = self.nodes.write().await;
        for (key, level_nodes) in nodes.iter_mut() {
            if key.starts_with(&prefix) {
                level_nodes.retain(|n| n.source_uri != source_uri);
            }
        }
        Ok(())
    }

    #[instrument(skip(self), fields(store = "in_memory_tree"), err)]
    async fn list_collections(&self) -> Result<Vec<String>> {
        let created = self.created.read().await;
        let nodes = self.nodes.read().await;
        let mut all: std::collections::HashSet<String> = created.clone();
        for key in nodes.keys() {
            if let Some(col) = key.split(':').next() {
                all.insert(col.to_string());
            }
        }
        let mut result: Vec<String> = all.into_iter().collect();
        result.sort();
        Ok(result)
    }

    #[instrument(skip(self), fields(store = "in_memory_tree", collection = collection), err)]
    async fn create_collection(&self, collection: &str) -> Result<()> {
        let mut created = self.created.write().await;
        let nodes = self.nodes.read().await;
        let key_prefix = format!("{}:", collection);
        let in_nodes = nodes.keys().any(|k| k.starts_with(&key_prefix));
        if created.contains(collection) || in_nodes {
            return Err(ArcanumError::AlreadyExists(
                format!("collection '{}' already exists", collection),
            ));
        }
        created.insert(collection.to_string());
        Ok(())
    }

    #[instrument(skip(self), fields(store = "in_memory_tree", collection = ?collection), err)]
    async fn count_documents(&self, collection: Option<&str>) -> Result<u64> {
        let nodes = self.nodes.read().await;
        let mut uris = std::collections::HashSet::new();
        for (key, node_list) in nodes.iter() {
            let matches = match collection {
                Some(col) => key.starts_with(&format!("{}:", col)),
                None => true,
            };
            if matches {
                for node in node_list {
                    if !node.source_uri.is_empty() {
                        uris.insert(node.source_uri.clone());
                    }
                }
            }
        }
        Ok(uris.len() as u64)
    }

    async fn get_by_id(&self, node_id: &TreeNodeId) -> arcanum_core::Result<Option<TreeNode>> {
        let nodes = self.nodes.read().await;
        Ok(nodes
            .values()
            .flat_map(|v| v.iter())
            .find(|n| n.id.0 == node_id.0)
            .cloned())
    }

    #[instrument(skip(self), fields(store = "in_memory_tree", collection = collection), err)]
    async fn delete_collection(&self, collection: &str) -> Result<()> {
        let prefix = format!("{}:", collection);
        self.nodes.write().await.retain(|k, _| !k.starts_with(&prefix));
        self.created.write().await.remove(collection);
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
            leaf_chunk_ids: vec![],
        }).await.unwrap();
        store.insert_node("col", TreeNode {
            id: TreeNodeId::new(), level: 0, text: "chunk b".into(),
            vector: Vector(vec![0.2]), parent: None, children: vec![],
            cluster_centroid: None, source_uri: "file://b.md".into(),
            leaf_chunk_ids: vec![],
        }).await.unwrap();
        store.delete_by_source_uri("col", "file://a.md").await.unwrap();
        let nodes = store.get_level("col", 0).await.unwrap();
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].source_uri, "file://b.md");
    }

    #[tokio::test]
    async fn test_in_memory_get_by_id_returns_inserted_node() {
        let store = InMemoryTreeStore::new();
        let node = TreeNode {
            id:               TreeNodeId::new(),
            level:            0,
            text:             "hello".into(),
            vector:           Vector(vec![0.1, 0.2]),
            parent:           None,
            children:         vec![],
            cluster_centroid: None,
            source_uri:       "file://doc.pdf".into(),
            leaf_chunk_ids:   vec![],
        };
        let id = node.id.clone();
        store.insert_node("col", node).await.unwrap();
        let found = store.get_by_id(&id).await.unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().id.0, id.0);
    }

    #[tokio::test]
    async fn test_in_memory_get_by_id_returns_none_for_missing() {
        let store = InMemoryTreeStore::new();
        let found = store.get_by_id(&TreeNodeId::new()).await.unwrap();
        assert!(found.is_none());
    }
}
