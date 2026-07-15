use arcanum_core::{traits::*, types::*, Result};
use async_trait::async_trait;
use std::sync::Arc;
use tracing::instrument;
use uuid::Uuid;

/// RAPTOR retriever: queries hierarchical tree levels (coarse→fine), scoring
/// each node with cosine similarity to the query vector, weighted by level
/// (lower levels = leaf = higher weight).
pub struct RaptorRetriever {
    tree_store: Arc<dyn TreeStore>,
    embedder: Arc<dyn Embedder>,
    max_depth: usize,
}

/// Fixed namespace for deriving a stable DocumentId from a TreeNode's
/// source_uri, so multiple nodes from the same source document collapse
/// onto one DocumentId instead of each getting a fresh random one — see
/// the doc_id derivation in `retrieve` below.
const SOURCE_URI_NAMESPACE: Uuid = Uuid::from_bytes([
    0xa1, 0x1c, 0xa4, 0x4e, 0x6d, 0x0e, 0x4c, 0x0b,
    0x9c, 0x1e, 0x52, 0x2e, 0xf3, 0x0a, 0x9b, 0x7d,
]);

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
            // Deterministic per source_uri so multiple chunks/nodes from the
            // same document share a DocumentId (needed for reduce_to_best_per_doc
            // and cross-strategy fusion to recognize them as one document);
            // falls back to the node's own id when source_uri is unset so we
            // don't collapse every untitled node onto the same DocumentId.
            let doc_id = if node.source_uri.is_empty() {
                DocumentId(node.id.0)
            } else {
                DocumentId(Uuid::new_v5(&SOURCE_URI_NAMESPACE, node.source_uri.as_bytes()))
            };
            RetrievedChunk {
                indexed_chunk: IndexedChunk {
                    chunk: Chunk {
                        id: ChunkId::new(),
                        text: node.text,
                        document_id: doc_id,
                        collection_id: collection_id.clone(),
                        position: ChunkPosition { start: 0, end: 0, index: node.level as usize },
                        metadata: ChunkMetadata::default(),
                        provenance: Default::default(),
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
        async fn delete_by_source_uri(&self, _: &str, _: &str) -> Result<()> { Ok(()) }
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

    #[tokio::test]
    async fn nodes_from_the_same_source_share_a_document_id() {
        let mock = MockTreeStore(Mutex::new(HashMap::new()));
        let same_source = TreeNode {
            id: TreeNodeId::new(), level: 0, text: "a".into(), vector: Vector(vec![0.1, 0.2, 0.3]),
            parent: None, children: vec![], cluster_centroid: None,
            source_uri: "file:///doc.pdf".into(), leaf_chunk_ids: vec![],
        };
        let other_node_same_source = TreeNode {
            id: TreeNodeId::new(), level: 0, text: "b".into(), vector: Vector(vec![0.1, 0.2, 0.3]),
            parent: None, children: vec![], cluster_centroid: None,
            source_uri: "file:///doc.pdf".into(), leaf_chunk_ids: vec![],
        };
        let different_source = TreeNode {
            id: TreeNodeId::new(), level: 0, text: "c".into(), vector: Vector(vec![0.1, 0.2, 0.3]),
            parent: None, children: vec![], cluster_centroid: None,
            source_uri: "file:///other.pdf".into(), leaf_chunk_ids: vec![],
        };
        mock.insert_node("col", same_source).await.unwrap();
        mock.insert_node("col", other_node_same_source).await.unwrap();
        mock.insert_node("col", different_source).await.unwrap();

        let store: Arc<dyn TreeStore> = Arc::new(mock);
        let embedder: Arc<dyn Embedder> = Arc::new(MockEmbedder);
        let retriever = RaptorRetriever::new(store, embedder, 0);
        let query = Query::new("q").with_collection(CollectionId("col".into())).with_top_k(3);
        let results = retriever.retrieve(&query).await.unwrap();

        assert_eq!(results.len(), 3);
        let doc_id = |text: &str| results.iter()
            .find(|r| r.indexed_chunk.chunk.text == text)
            .unwrap().indexed_chunk.chunk.document_id.clone();
        assert_eq!(doc_id("a"), doc_id("b"), "same source_uri must yield the same document_id");
        assert_ne!(doc_id("a"), doc_id("c"), "different source_uri must yield different document_ids");
    }
}
