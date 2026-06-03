use arcanum_core::{traits::*, types::*, Result};
use async_trait::async_trait;
use std::sync::Arc;
use tracing::instrument;

pub struct VectorRetriever {
    store: Arc<dyn VectorStore>,
    embedder: Arc<dyn Embedder>,
}

impl VectorRetriever {
    pub fn new(store: Arc<dyn VectorStore>, embedder: Arc<dyn Embedder>) -> Self {
        Self { store, embedder }
    }
}

#[async_trait]
impl Retriever for VectorRetriever {
    #[instrument(skip(self), fields(strategy = "vector", collection_id = ?query.collection_id, top_k = ?query.top_k), err)]
    async fn retrieve(&self, query: &Query) -> Result<Vec<RetrievedChunk>> {
        let vectors = self.embedder.embed(vec![query.text.clone()]).await?;
        // Require explicit collection_id — fail-open fallback would allow cross-collection access.
        let collection_id = query.collection_id.as_ref()
            .ok_or_else(|| arcanum_core::ArcanumError::Config(
                "VectorRetriever requires an explicit collection_id".into()
            ))?;
        let collection = collection_id.0.as_str();
        let results = self.store.search(collection, &VectorQuery {
            vector: vectors.into_iter().next().unwrap_or(Vector(vec![])),
            top_k: query.top_k,
            filters: query.filters.clone(),
        }).await?;
        Ok(results.into_iter().map(|s| RetrievedChunk {
            indexed_chunk: s.chunk, score: s.score,
            strategy: RetrievalStrategy::Vector,
        }).collect())
    }

    fn strategy(&self) -> RetrievalStrategy { RetrievalStrategy::Vector }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    struct MockVectorStore(Mutex<HashMap<String, Vec<IndexedChunk>>>);

    #[async_trait::async_trait]
    impl VectorStore for MockVectorStore {
        async fn upsert(&self, collection: &str, chunks: Vec<IndexedChunk>) -> Result<()> {
            self.0.lock().unwrap().entry(collection.to_string()).or_default().extend(chunks);
            Ok(())
        }
        async fn search(&self, collection: &str, query: &VectorQuery) -> Result<Vec<ScoredChunk>> {
            let store = self.0.lock().unwrap();
            let chunks = store.get(collection).cloned().unwrap_or_default();
            Ok(chunks.into_iter().take(query.top_k).map(|c| ScoredChunk { chunk: c, score: 0.9 }).collect())
        }
        async fn delete(&self, _collection: &str, _ids: &[ChunkId]) -> Result<()> { Ok(()) }
        async fn collection_exists(&self, collection: &str) -> Result<bool> {
            Ok(self.0.lock().unwrap().contains_key(collection))
        }
    }

    struct MockEmbedder;
    #[async_trait::async_trait]
    impl Embedder for MockEmbedder {
        async fn embed(&self, texts: Vec<String>) -> Result<Vec<Vector>> {
            Ok(texts.iter().map(|_| Vector(vec![0.1, 0.2, 0.3])).collect())
        }
        fn dimension(&self) -> usize { 3 }
    }

    #[tokio::test]
    async fn test_vector_retriever_accepts_arc_dyn_vector_store() {
        // This test verifies VectorRetriever accepts Arc<dyn VectorStore>, not LanceDbStore directly.
        let store: Arc<dyn VectorStore> = Arc::new(MockVectorStore(Mutex::new(HashMap::new())));
        let embedder: Arc<dyn Embedder> = Arc::new(MockEmbedder);
        let retriever = VectorRetriever::new(store, embedder);
        assert_eq!(retriever.strategy(), RetrievalStrategy::Vector);
    }

    #[tokio::test]
    async fn test_vector_retriever_retrieve() {
        let store: Arc<dyn VectorStore> = Arc::new(MockVectorStore(Mutex::new(HashMap::new())));
        let embedder: Arc<dyn Embedder> = Arc::new(MockEmbedder);
        let retriever = VectorRetriever::new(store, embedder);
        let query = Query::new("test").with_collection(CollectionId("col".into()));
        let results = retriever.retrieve(&query).await.unwrap();
        assert!(results.is_empty()); // empty store
    }
}
