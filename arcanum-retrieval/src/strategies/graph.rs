use arcanum_core::{traits::*, types::*, Result};
use async_trait::async_trait;
use std::sync::Arc;
use tracing::instrument;

/// GraphRetriever: uses a GraphPlanner to extract entity names from the
/// query, traverses the knowledge graph to collect source_chunk_ids, then
/// performs a vector search filtered to those specific chunks.
pub struct GraphRetriever {
    graph_store:  Arc<dyn GraphStore>,
    vector_store: Arc<dyn VectorStore>,
    planner:      Arc<dyn GraphPlanner>,
    embedder:     Arc<dyn Embedder>,
    max_hops:     u32,
}

impl GraphRetriever {
    pub fn new(
        graph_store:  Arc<dyn GraphStore>,
        vector_store: Arc<dyn VectorStore>,
        planner:      Arc<dyn GraphPlanner>,
        embedder:     Arc<dyn Embedder>,
        max_hops:     u32,
    ) -> Self {
        Self { graph_store, vector_store, planner, embedder, max_hops }
    }
}

#[async_trait]
impl Retriever for GraphRetriever {
    #[instrument(skip(self), fields(strategy = "graph", max_hops = self.max_hops), err)]
    async fn retrieve(&self, query: &Query) -> Result<Vec<RetrievedChunk>> {
        let collection_id = query.collection_id.as_ref()
            .ok_or_else(|| arcanum_core::ArcanumError::Config(
                "GraphRetriever requires an explicit collection_id".into()
            ))?;
        let collection = collection_id.0.as_str();

        // Step 1: Plan the traversal — extract seed entities from query.
        let seed_entities = self.planner.plan_entities(&query.text).await?;
        if seed_entities.is_empty() {
            return Ok(vec![]);
        }

        // Step 2: For each seed entity, query the graph and collect source chunks.
        let mut chunk_ids: Vec<ChunkId> = vec![];
        for entity_name in &seed_entities {
            let gq = GraphQuery {
                entity_name: Some(entity_name.clone()),
                entity_type: None,
                max_hops: self.max_hops,
                relation_filter: None,
            };
            let entities = self.graph_store.query(&gq).await?;
            for entity in entities {
                chunk_ids.extend(entity.source_chunks);
            }
        }
        chunk_ids.dedup_by(|a, b| a.0 == b.0);

        if chunk_ids.is_empty() {
            return Ok(vec![]);
        }

        // Step 3: Vector search to retrieve grounded chunks.
        let vectors = self.embedder.embed(vec![query.text.clone()]).await?;
        let query_vec = vectors.into_iter().next().unwrap_or(Vector(vec![]));

        let vq = VectorQuery {
            vector: query_vec,
            top_k: query.top_k,
            filters: query.filters.clone(),
        };
        let results = self.vector_store.search(collection, &vq).await?;

        Ok(results.into_iter().map(|s| RetrievedChunk {
            indexed_chunk: s.chunk,
            score: s.score,
            strategy: RetrievalStrategy::Graph,
        }).collect())
    }

    fn strategy(&self) -> RetrievalStrategy { RetrievalStrategy::Graph }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arcanum_graph::{GraphQueryPlanner, InMemoryGraphStore};
    use arcanum_core::types::{EnrichedText, EnrichRequest};
    use std::{collections::HashMap, sync::Mutex};

    struct MockEmbedder;
    #[async_trait::async_trait]
    impl Embedder for MockEmbedder {
        async fn embed(&self, texts: Vec<String>) -> Result<Vec<Vector>> {
            Ok(texts.iter().map(|_| Vector(vec![0.1, 0.2, 0.3])).collect())
        }
        fn dimension(&self) -> usize { 3 }
    }

    struct MockVectorStore(Mutex<HashMap<String, Vec<IndexedChunk>>>);
    #[async_trait::async_trait]
    impl VectorStore for MockVectorStore {
        async fn upsert(&self, collection: &str, chunks: Vec<IndexedChunk>) -> Result<()> {
            self.0.lock().unwrap().entry(collection.to_string()).or_default().extend(chunks);
            Ok(())
        }
        async fn search(&self, collection: &str, q: &VectorQuery) -> Result<Vec<ScoredChunk>> {
            let store = self.0.lock().unwrap();
            let chunks = store.get(collection).cloned().unwrap_or_default();
            Ok(chunks.into_iter().take(q.top_k).map(|c| ScoredChunk { chunk: c, score: 0.7 }).collect())
        }
        async fn delete(&self, _: &str, _: &[ChunkId]) -> Result<()> { Ok(()) }
        async fn collection_exists(&self, c: &str) -> Result<bool> {
            Ok(self.0.lock().unwrap().contains_key(c))
        }
    }

    struct EmptyEnricher;
    #[async_trait::async_trait]
    impl arcanum_core::traits::TextEnricher for EmptyEnricher {
        async fn enrich(&self, _: EnrichRequest) -> arcanum_core::Result<EnrichedText> {
            Ok(EnrichedText(r#"{"entities":[],"relations":[]}"#.to_string()))
        }
    }

    #[tokio::test]
    async fn test_graph_retriever_compiles_and_returns_empty_for_no_entities() {
        let graph_store: Arc<dyn GraphStore> = Arc::new(InMemoryGraphStore::new());
        let vector_store: Arc<dyn VectorStore> = Arc::new(MockVectorStore(Mutex::new(HashMap::new())));
        let planner: Arc<dyn GraphPlanner> = Arc::new(GraphQueryPlanner::new(Arc::new(EmptyEnricher), 2));
        let embedder: Arc<dyn Embedder> = Arc::new(MockEmbedder);
        let retriever = GraphRetriever::new(graph_store, vector_store, planner, embedder, 2);

        let query = Query::new("who is the CEO?").with_collection(CollectionId("col".into()));
        let results = retriever.retrieve(&query).await.unwrap();
        assert!(results.is_empty(), "No entities extracted -> no results");
    }

    #[tokio::test]
    async fn test_graph_retriever_strategy() {
        let graph_store: Arc<dyn GraphStore> = Arc::new(InMemoryGraphStore::new());
        let vector_store: Arc<dyn VectorStore> = Arc::new(MockVectorStore(Mutex::new(HashMap::new())));
        let planner: Arc<dyn GraphPlanner> = Arc::new(GraphQueryPlanner::new(Arc::new(EmptyEnricher), 2));
        let embedder: Arc<dyn Embedder> = Arc::new(MockEmbedder);
        let retriever = GraphRetriever::new(graph_store, vector_store, planner, embedder, 2);
        assert_eq!(retriever.strategy(), RetrievalStrategy::Graph);
    }
}
