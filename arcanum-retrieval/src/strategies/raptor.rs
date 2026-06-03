use arcanum_core::{traits::*, types::*, Result};
use async_trait::async_trait;
use std::sync::Arc;
use tracing::instrument;

/// RAPTOR retriever: queries hierarchical tree levels (coarse→fine), scoring
/// each node with cosine similarity to the query vector, weighted by level
/// (lower levels = leaf = higher weight).
pub struct RaptorRetriever {
    tree_store: Arc<dyn TreeStore>,
    embedder: Arc<dyn Embedder>,
    max_depth: usize,
}

impl RaptorRetriever {
    pub fn new(
        tree_store: Arc<dyn TreeStore>,
        embedder: Arc<dyn Embedder>,
        max_depth: usize,
    ) -> Self {
        Self { tree_store, embedder, max_depth }
    }

    fn cosine(a: &[f32], b: &[f32]) -> f32 {
        if a.len() != b.len() || a.is_empty() { return 0.0; }
        let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
        let na: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
        let nb: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
        if na == 0.0 || nb == 0.0 { 0.0 } else { dot / (na * nb) }
    }
}

#[async_trait]
impl Retriever for RaptorRetriever {
    #[instrument(skip(self), fields(strategy = "raptor", max_depth = self.max_depth), err)]
    async fn retrieve(&self, query: &Query) -> Result<Vec<RetrievedChunk>> {
        let collection_id = query.collection_id.as_ref()
            .ok_or_else(|| arcanum_core::ArcanumError::Config(
                "RaptorRetriever requires an explicit collection_id".into()
            ))?;
        let collection = collection_id.0.as_str();

        let vectors = self.embedder.embed(vec![query.text.clone()]).await?;
        let query_vec = vectors.into_iter().next().unwrap_or(Vector(vec![]));

        let mut candidates: Vec<(f32, TreeNode)> = vec![];

        // Traverse from deepest level down to level 0 (level 0 = leaves).
        for depth in 0..=self.max_depth {
            let level = self.max_depth.saturating_sub(depth) as u32;
            let nodes = self.tree_store.get_level(collection, level).await?;
            // Weight: leaf nodes (level 0) get weight 1.0, higher levels get lower weight.
            let level_weight = 1.0 / (1.0 + level as f32);
            for node in nodes {
                let sim = Self::cosine(&query_vec.0, &node.vector.0);
                candidates.push((sim * level_weight, node));
            }
        }

        candidates.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        candidates.truncate(query.top_k);

        Ok(candidates.into_iter().map(|(score, node)| {
            let doc_id = DocumentId::new();
            RetrievedChunk {
                indexed_chunk: IndexedChunk {
                    chunk: Chunk {
                        id: ChunkId::new(),
                        text: node.text,
                        document_id: doc_id,
                        collection_id: collection_id.clone(),
                        position: ChunkPosition { start: 0, end: 0, index: node.level as usize },
                        metadata: ChunkMetadata::default(),
                    },
                    vector: node.vector,
                    token_vectors: None,
                    store_id: node.id.0.to_string(),
                },
                score,
                strategy: RetrievalStrategy::Raptor,
            }
        }).collect())
    }

    fn strategy(&self) -> RetrievalStrategy { RetrievalStrategy::Raptor }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{collections::HashMap, sync::Mutex};

    struct MockEmbedder;
    #[async_trait::async_trait]
    impl Embedder for MockEmbedder {
        async fn embed(&self, texts: Vec<String>) -> Result<Vec<Vector>> {
            Ok(texts.iter().map(|_| Vector(vec![0.1, 0.2, 0.3])).collect())
        }
        fn dimension(&self) -> usize { 3 }
    }

    struct MockTreeStore(Mutex<HashMap<String, Vec<TreeNode>>>);
    #[async_trait::async_trait]
    impl TreeStore for MockTreeStore {
        async fn insert_node(&self, collection: &str, node: TreeNode) -> Result<()> {
            let key = format!("{}:{}", collection, node.level);
            self.0.lock().unwrap().entry(key).or_default().push(node);
            Ok(())
        }
        async fn get_level(&self, collection: &str, level: u32) -> Result<Vec<TreeNode>> {
            let key = format!("{}:{}", collection, level);
            Ok(self.0.lock().unwrap().get(&key).cloned().unwrap_or_default())
        }
        async fn get_children(&self, _node_id: &TreeNodeId) -> Result<Vec<TreeNode>> {
            Ok(vec![])
        }
    }

    #[tokio::test]
    async fn test_raptor_retriever_empty_tree() {
        let store: Arc<dyn TreeStore> = Arc::new(MockTreeStore(Mutex::new(HashMap::new())));
        let embedder: Arc<dyn Embedder> = Arc::new(MockEmbedder);
        let retriever = RaptorRetriever::new(store, embedder, 3);
        let query = Query::new("summarize the document").with_collection(CollectionId("col".into()));
        let results = retriever.retrieve(&query).await.unwrap();
        assert!(results.is_empty(), "Empty tree should return no results");
    }

    #[tokio::test]
    async fn test_raptor_retriever_strategy() {
        let store: Arc<dyn TreeStore> = Arc::new(MockTreeStore(Mutex::new(HashMap::new())));
        let embedder: Arc<dyn Embedder> = Arc::new(MockEmbedder);
        let retriever = RaptorRetriever::new(store, embedder, 2);
        assert_eq!(retriever.strategy(), RetrievalStrategy::Raptor);
    }
}
