use crate::hydrate::fetch_records;
use arcanum_core::{traits::*, types::*, Result};
use async_trait::async_trait;
use std::sync::Arc;
use tracing::{instrument, warn};

/// RAPTOR retriever: queries hierarchical tree levels (coarse→fine), scoring
/// each node with cosine similarity to the query vector, weighted by level
/// (lower levels = leaf = higher weight).
pub struct RaptorRetriever {
    tree_store: Arc<dyn TreeStore>,
    embedder: Arc<dyn Embedder>,
    chunk_metadata: Arc<dyn ChunkMetadataStore>,
    max_depth: usize,
}

impl RaptorRetriever {
    pub fn new(
        tree_store: Arc<dyn TreeStore>,
        embedder: Arc<dyn Embedder>,
        chunk_metadata: Arc<dyn ChunkMetadataStore>,
        max_depth: usize,
    ) -> Self {
        Self {
            tree_store,
            embedder,
            chunk_metadata,
            max_depth,
        }
    }

    fn cosine(a: &[f32], b: &[f32]) -> f32 {
        if a.len() != b.len() || a.is_empty() {
            return 0.0;
        }
        let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
        let na: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
        let nb: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
        if na == 0.0 || nb == 0.0 {
            0.0
        } else {
            dot / (na * nb)
        }
    }
}

#[async_trait]
impl Retriever for RaptorRetriever {
    #[instrument(skip(self), fields(strategy = "raptor", max_depth = self.max_depth), err)]
    async fn retrieve(&self, query: &Query) -> Result<Vec<RetrievedChunk>> {
        let collection_id = query.collection_id.as_ref().ok_or_else(|| {
            arcanum_core::ArcanumError::Config(
                "RaptorRetriever requires an explicit collection_id".into(),
            )
        })?;
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

        // Every node is anchored to the registry through its first leaf chunk id:
        // leaves resolve to their own chunk, summaries borrow the document id.
        let ids: Vec<ChunkId> = candidates
            .iter()
            .filter_map(|(_, node)| node.leaf_chunk_ids.first().cloned())
            .collect();
        let records = fetch_records(
            self.chunk_metadata.as_ref(),
            &ids,
            collection,
            &RetrievalStrategy::Raptor,
        )
        .await?;

        let mut results = Vec::with_capacity(candidates.len());
        for (score, node) in candidates {
            let Some(first) = node.leaf_chunk_ids.first() else {
                warn!(
                    collection,
                    strategy = "raptor",
                    node_id = %node.id.0,
                    "tree node has no leaf chunk ids"
                );
                metrics::counter!(
                    "arcanum_retrieval_unresolved_chunks_total",
                    "strategy" => "raptor"
                )
                .increment(1);
                continue;
            };
            let Some(record) = records.get(first) else {
                continue;
            };
            let (chunk, kind) = if node.level == 0 {
                (record.to_chunk(), ChunkKind::Source)
            } else {
                (
                    Chunk {
                        id: ChunkId(node.id.0),
                        text: node.text,
                        document_id: record.document_id.clone(),
                        collection_id: collection_id.clone(),
                        position: ChunkPosition {
                            start: 0,
                            end: 0,
                            index: 0,
                        },
                        metadata: ChunkMetadata::default(),
                        provenance: Default::default(),
                    },
                    ChunkKind::Summary {
                        level: node.level,
                        covers: node.leaf_chunk_ids,
                    },
                )
            };
            results.push(RetrievedChunk {
                indexed_chunk: IndexedChunk {
                    chunk,
                    vector: node.vector,
                    token_vectors: None,
                    store_id: node.id.0.to_string(),
                },
                score,
                strategy: RetrievalStrategy::Raptor,
                kind,
            });
        }
        Ok(results)
    }

    fn strategy(&self) -> RetrievalStrategy {
        RetrievalStrategy::Raptor
    }
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
        fn dimension(&self) -> usize {
            3
        }
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
            Ok(self
                .0
                .lock()
                .unwrap()
                .get(&key)
                .cloned()
                .unwrap_or_default())
        }
        async fn get_children(&self, _node_id: &TreeNodeId) -> Result<Vec<TreeNode>> {
            Ok(vec![])
        }
        async fn delete_by_source_uri(&self, _: &str, _: &str) -> Result<()> {
            Ok(())
        }
    }

    fn registry() -> Arc<InMemoryChunkMetadataStore> {
        Arc::new(InMemoryChunkMetadataStore::new())
    }

    fn record(text: &str, document_id: DocumentId) -> ChunkMetadataRecord {
        ChunkMetadataRecord {
            chunk_id: ChunkId::new(),
            document_id,
            collection_id: "col".into(),
            version_num: 1,
            backend: ChunkBackend::Tree,
            text: text.into(),
            chunk_index: 3,
            source_uri: "file:///doc.pdf".into(),
            snapshot_uri: "file:///snap/doc/1.raw".into(),
            canonical_uri: None,
            page: None,
            section: None,
            block_ids: vec![],
            offset_start: 7,
            offset_end: 7 + text.len(),
            ingested_at: chrono::Utc::now(),
        }
    }

    fn node(level: u32, text: &str, leaf_chunk_ids: Vec<ChunkId>) -> TreeNode {
        TreeNode {
            id: TreeNodeId::new(),
            level,
            text: text.into(),
            vector: Vector(vec![0.1, 0.2, 0.3]),
            parent: None,
            children: vec![],
            cluster_centroid: None,
            source_uri: "file:///doc.pdf".into(),
            leaf_chunk_ids,
        }
    }

    fn retriever(
        store: MockTreeStore,
        chunk_metadata: Arc<InMemoryChunkMetadataStore>,
        max_depth: usize,
    ) -> RaptorRetriever {
        RaptorRetriever::new(
            Arc::new(store),
            Arc::new(MockEmbedder),
            chunk_metadata,
            max_depth,
        )
    }

    fn query(top_k: usize) -> Query {
        Query::new("q")
            .with_collection(CollectionId("col".into()))
            .with_top_k(top_k)
    }

    #[tokio::test]
    async fn test_raptor_retriever_empty_tree() {
        let r = retriever(MockTreeStore(Mutex::new(HashMap::new())), registry(), 3);
        let query =
            Query::new("summarize the document").with_collection(CollectionId("col".into()));
        let results = r.retrieve(&query).await.unwrap();
        assert!(results.is_empty(), "Empty tree should return no results");
    }

    #[tokio::test]
    async fn test_raptor_retriever_strategy() {
        let r = retriever(MockTreeStore(Mutex::new(HashMap::new())), registry(), 2);
        assert_eq!(r.strategy(), RetrievalStrategy::Raptor);
    }

    #[tokio::test]
    async fn leaf_results_use_registry_chunk() {
        let rec = record("leaf source text", DocumentId::new());
        let reg = registry();
        reg.put(&rec).await.unwrap();
        let mock = MockTreeStore(Mutex::new(HashMap::new()));
        let leaf = node(0, "leaf source text", vec![rec.chunk_id.clone()]);
        let leaf_id = leaf.id.clone();
        mock.insert_node("col", leaf).await.unwrap();

        let results = retriever(mock, reg, 0).retrieve(&query(5)).await.unwrap();
        assert_eq!(results.len(), 1);
        let r = &results[0];
        assert_eq!(r.kind, ChunkKind::Source);
        assert_eq!(r.indexed_chunk.store_id, leaf_id.0.to_string());
        let c = &r.indexed_chunk.chunk;
        assert_eq!(c.id, rec.chunk_id);
        assert_eq!(c.document_id, rec.document_id);
        assert_eq!(c.position.start, 7);
        assert_eq!(c.position.index, 3);
    }

    #[tokio::test]
    async fn summary_results_are_typed_with_covers_and_document_id() {
        let doc_id = DocumentId::new();
        let a = record("first leaf", doc_id.clone());
        let b = record("second leaf", doc_id.clone());
        let reg = registry();
        reg.put(&a).await.unwrap();
        reg.put(&b).await.unwrap();
        let mock = MockTreeStore(Mutex::new(HashMap::new()));
        let summary = node(
            1,
            "summary text",
            vec![a.chunk_id.clone(), b.chunk_id.clone()],
        );
        let summary_id = summary.id.clone();
        mock.insert_node("col", summary).await.unwrap();

        let results = retriever(mock, reg, 1).retrieve(&query(5)).await.unwrap();
        assert_eq!(results.len(), 1);
        let r = &results[0];
        assert_eq!(
            r.kind,
            ChunkKind::Summary {
                level: 1,
                covers: vec![a.chunk_id.clone(), b.chunk_id.clone()],
            }
        );
        let c = &r.indexed_chunk.chunk;
        assert_eq!(c.id, ChunkId(summary_id.0));
        assert_eq!(c.text, "summary text");
        assert_eq!(c.document_id, doc_id);
        assert_eq!(c.collection_id, CollectionId("col".into()));
        assert_eq!(c.position.start, 0);
        assert_eq!(c.position.end, 0);
    }

    #[tokio::test]
    async fn node_with_unregistered_leaf_is_skipped() {
        let known = record("known", DocumentId::new());
        let reg = registry();
        reg.put(&known).await.unwrap();
        let mock = MockTreeStore(Mutex::new(HashMap::new()));
        mock.insert_node("col", node(0, "known", vec![known.chunk_id.clone()]))
            .await
            .unwrap();
        mock.insert_node("col", node(0, "ghost", vec![ChunkId::new()]))
            .await
            .unwrap();
        mock.insert_node("col", node(0, "no leaves", vec![]))
            .await
            .unwrap();

        let results = retriever(mock, reg, 0).retrieve(&query(5)).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].indexed_chunk.chunk.text, "known");
    }
}
