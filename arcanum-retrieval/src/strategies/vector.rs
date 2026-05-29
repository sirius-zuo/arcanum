use arcanum_core::{traits::*, types::*, Result};
use arcanum_vector::LanceDbStore;
use async_trait::async_trait;
use std::sync::Arc;

pub struct VectorRetriever {
    store: Arc<LanceDbStore>,
    embedder: Arc<dyn Embedder>,
}

impl VectorRetriever {
    pub fn new(store: Arc<LanceDbStore>, embedder: Arc<dyn Embedder>) -> Self {
        Self { store, embedder }
    }
}

#[async_trait]
impl Retriever for VectorRetriever {
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
