use crate::Result;
use async_trait::async_trait;

/// Lexical (BM25-style) full-text search over a single collection.
/// Returns (chunk_store_id, score) pairs sorted by score descending.
#[async_trait]
pub trait LexicalIndex: Send + Sync {
    async fn search(
        &self,
        collection_id: &str,
        query: &str,
        top_k: usize,
    ) -> Result<Vec<(String, f32)>>;
}
