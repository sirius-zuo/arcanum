use arcanum_core::{traits::*, types::*, Result, ArcanumError};
use async_trait::async_trait;
use std::sync::Arc;
use tracing::instrument;

/// BM25 retriever scoped to a single collection.
///
/// Each instance owns one collection's index. Requests for a different
/// collection_id are denied, preventing cross-collection data leakage.
///
/// KNOWN LIMITATION: Tantivy's search returns only the stored `id` string
/// (the chunk's store_id), not a full IndexedChunk. Until a metadata lookup
/// is wired in (via SqliteMetadataStore), the returned chunks have a stub
/// DocumentId derived from the store_id rather than the canonical one.
/// Do NOT use the returned DocumentId/ChunkId as authoritative identifiers
/// until this is resolved.
pub struct Bm25Retriever {
    collection_id: Option<CollectionId>,   // None = accept any collection
    index: Arc<dyn LexicalIndex>,
}

impl Bm25Retriever {
    /// Collection-scoped: rejects queries for other collections.
    pub fn new(collection_id: CollectionId, index: Arc<dyn LexicalIndex>) -> Self {
        Self { collection_id: Some(collection_id), index }
    }

    /// Global: serves any collection (index is not partitioned by collection).
    pub fn new_global(index: Arc<dyn LexicalIndex>) -> Self {
        Self { collection_id: None, index }
    }
}

#[async_trait]
impl Retriever for Bm25Retriever {
    #[instrument(skip(self), fields(strategy = "bm25"), err)]
    async fn retrieve(&self, query: &Query) -> Result<Vec<RetrievedChunk>> {
        let query_cid = query.collection_id.as_ref()
            .ok_or_else(|| ArcanumError::Config(
                "Bm25Retriever requires an explicit collection_id".into()
            ))?;

        if let Some(scope) = &self.collection_id {
            if scope.0 != query_cid.0 {
                return Err(ArcanumError::Config(format!(
                    "Bm25Retriever for '{}' cannot serve collection '{}'",
                    scope.0, query_cid.0
                )));
            }
        }

        let raw = self.index.search(&query_cid.0, &query.text, query.top_k).await?;
        let collection_id = query_cid.clone();
        Ok(raw.into_iter().map(|(store_id, score)| RetrievedChunk {
            indexed_chunk: IndexedChunk {
                chunk: Chunk {
                    id: ChunkId::new(),
                    text: store_id.clone(),
                    document_id: DocumentId::new(),
                    collection_id: collection_id.clone(),
                    position: ChunkPosition { start: 0, end: 0, index: 0 },
                    metadata: ChunkMetadata::default(),
                    provenance: Default::default(),
                },
                vector: Vector(vec![]), token_vectors: None, store_id,
            },
            score, strategy: RetrievalStrategy::Bm25,
        }).collect())
    }

    fn strategy(&self) -> RetrievalStrategy { RetrievalStrategy::Bm25 }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FakeLexicalIndex {
        hits: Vec<(String, f32)>,
    }
    #[async_trait::async_trait]
    impl LexicalIndex for FakeLexicalIndex {
        async fn search(
            &self,
            _collection_id: &str,
            _query: &str,
            _top_k: usize,
        ) -> arcanum_core::Result<Vec<(String, f32)>> {
            Ok(self.hits.clone())
        }
    }

    #[tokio::test]
    async fn test_bm25_retriever_uses_lexical_index_trait() {
        let index: Arc<dyn LexicalIndex> = Arc::new(FakeLexicalIndex {
            hits: vec![("chunk-1".to_string(), 0.9)],
        });
        let retriever = Bm25Retriever::new(CollectionId("col1".into()), index);
        let query = Query::new("hello").with_collection(CollectionId("col1".into()));
        let result = retriever.retrieve(&query).await.unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].score, 0.9);
    }
}
