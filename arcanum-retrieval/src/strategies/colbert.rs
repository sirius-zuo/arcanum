use arcanum_core::{traits::*, types::*, Result};
use async_trait::async_trait;
use std::sync::Arc;

/// ColBERT-style retriever: coarse ANN pass, then MaxSim token re-scoring.
///
/// Step 1: embed the query as a single vector for coarse ANN retrieval.
/// Step 2: if the query has token_vectors available on indexed chunks, compute
///         MaxSim = sum over query tokens of max cosine similarity to any doc token.
///         Chunks that lack token_vectors fall back to the coarse ANN score.
pub struct ColBertRetriever {
    store: Arc<dyn VectorStore>,
    embedder: Arc<dyn Embedder>,
}

impl ColBertRetriever {
    pub fn new(store: Arc<dyn VectorStore>, embedder: Arc<dyn Embedder>) -> Self {
        Self { store, embedder }
    }

    /// Cosine similarity between two vectors (returns 0.0 if either is zero-length).
    fn cosine(a: &[f32], b: &[f32]) -> f32 {
        if a.len() != b.len() || a.is_empty() { return 0.0; }
        let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
        let na: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
        let nb: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
        if na == 0.0 || nb == 0.0 { 0.0 } else { dot / (na * nb) }
    }

    /// MaxSim scoring: for each query token vector, find the max cosine sim
    /// across all document token vectors, then sum those maxima.
    fn max_sim(query_tokens: &[Vector], doc_tokens: &[Vector]) -> f32 {
        if query_tokens.is_empty() || doc_tokens.is_empty() { return 0.0; }
        query_tokens.iter().map(|qt| {
            doc_tokens.iter()
                .map(|dt| Self::cosine(&qt.0, &dt.0))
                .fold(f32::NEG_INFINITY, f32::max)
        }).sum()
    }
}

#[async_trait]
impl Retriever for ColBertRetriever {
    async fn retrieve(&self, query: &Query) -> Result<Vec<RetrievedChunk>> {
        let collection_id = query.collection_id.as_ref()
            .ok_or_else(|| arcanum_core::ArcanumError::Config(
                "ColBertRetriever requires an explicit collection_id".into()
            ))?;
        let collection = collection_id.0.as_str();

        // Step 1: coarse ANN pass — embed query as single vector.
        let vectors = self.embedder.embed(vec![query.text.clone()]).await?;
        let query_vec = vectors.into_iter().next().unwrap_or(Vector(vec![]));

        // Fetch more candidates than top_k so MaxSim can rerank.
        let coarse_top_k = (query.top_k * 4).max(20);
        let coarse_vq = VectorQuery {
            vector: query_vec.clone(),
            top_k: coarse_top_k,
            filters: query.filters.clone(),
        };
        let candidates = self.store.search(collection, &coarse_vq).await?;

        // Step 2: MaxSim reranking. Use single query vector as a 1-token list
        // so behaviour degrades gracefully when no token_vectors are present.
        let query_tokens = vec![query_vec];
        let mut scored: Vec<(f32, IndexedChunk)> = candidates.into_iter().map(|s| {
            let score = match &s.chunk.token_vectors {
                Some(tv) if !tv.is_empty() => Self::max_sim(&query_tokens, tv),
                _ => s.score, // fallback to coarse score
            };
            (score, s.chunk)
        }).collect();

        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(query.top_k);

        Ok(scored.into_iter().map(|(score, chunk)| RetrievedChunk {
            indexed_chunk: chunk,
            score,
            strategy: RetrievalStrategy::ColBert,
        }).collect())
    }

    fn strategy(&self) -> RetrievalStrategy { RetrievalStrategy::ColBert }
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
            Ok(chunks.into_iter().take(query.top_k).map(|c| ScoredChunk { chunk: c, score: 0.8 }).collect())
        }
        async fn delete(&self, _: &str, _: &[ChunkId]) -> Result<()> { Ok(()) }
        async fn collection_exists(&self, c: &str) -> Result<bool> {
            Ok(self.0.lock().unwrap().contains_key(c))
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
    async fn test_colbert_retriever_strategy() {
        let store: Arc<dyn VectorStore> = Arc::new(MockVectorStore(Mutex::new(HashMap::new())));
        let embedder: Arc<dyn Embedder> = Arc::new(MockEmbedder);
        let retriever = ColBertRetriever::new(store, embedder);
        assert_eq!(retriever.strategy(), RetrievalStrategy::ColBert);
    }

    #[tokio::test]
    async fn test_colbert_retriever_empty_store() {
        let store: Arc<dyn VectorStore> = Arc::new(MockVectorStore(Mutex::new(HashMap::new())));
        let embedder: Arc<dyn Embedder> = Arc::new(MockEmbedder);
        let retriever = ColBertRetriever::new(store, embedder);
        let query = Query::new("who founded OpenAI?").with_collection(CollectionId("col".into()));
        let results = retriever.retrieve(&query).await.unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn test_max_sim_basic() {
        let q = vec![Vector(vec![1.0, 0.0])];
        let d = vec![Vector(vec![1.0, 0.0]), Vector(vec![0.0, 1.0])];
        let score = ColBertRetriever::max_sim(&q, &d);
        assert!((score - 1.0).abs() < 1e-5, "MaxSim should be 1.0 for identical vectors");
    }
}
