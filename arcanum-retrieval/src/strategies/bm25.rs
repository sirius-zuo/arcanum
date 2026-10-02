use crate::hydrate::hydrate;
use arcanum_core::{traits::*, types::*, ArcanumError, Result};
use async_trait::async_trait;
use std::sync::Arc;
use tracing::instrument;

/// BM25 retriever scoped to a single collection.
///
/// Each instance owns one collection's index. Requests for a different
/// collection_id are denied, preventing cross-collection data leakage.
///
/// The lexical index returns only scored chunk ids; text, document id and
/// provenance are hydrated from the chunk metadata registry.
pub struct Bm25Retriever {
    collection_id: Option<CollectionId>, // None = accept any collection
    index: Arc<dyn LexicalIndex>,
    chunk_metadata: Arc<dyn ChunkMetadataStore>,
}

impl Bm25Retriever {
    /// Collection-scoped: rejects queries for other collections.
    pub fn new(
        collection_id: CollectionId,
        index: Arc<dyn LexicalIndex>,
        chunk_metadata: Arc<dyn ChunkMetadataStore>,
    ) -> Self {
        Self {
            collection_id: Some(collection_id),
            index,
            chunk_metadata,
        }
    }

    /// Global: serves any collection (the index filters by collection per query).
    pub fn new_global(
        index: Arc<dyn LexicalIndex>,
        chunk_metadata: Arc<dyn ChunkMetadataStore>,
    ) -> Self {
        Self {
            collection_id: None,
            index,
            chunk_metadata,
        }
    }
}

#[async_trait]
impl Retriever for Bm25Retriever {
    #[instrument(skip(self), fields(strategy = "bm25"), err)]
    async fn retrieve(&self, query: &Query) -> Result<Vec<RetrievedChunk>> {
        let query_cid = query.collection_id.as_ref().ok_or_else(|| {
            ArcanumError::Config("Bm25Retriever requires an explicit collection_id".into())
        })?;

        if let Some(scope) = &self.collection_id {
            if scope.0 != query_cid.0 {
                return Err(ArcanumError::Config(format!(
                    "Bm25Retriever for '{}' cannot serve collection '{}'",
                    scope.0, query_cid.0
                )));
            }
        }

        let hits = self
            .index
            .search(&query_cid.0, &query.text, query.top_k)
            .await?;
        hydrate(
            self.chunk_metadata.as_ref(),
            &hits,
            &query_cid.0,
            RetrievalStrategy::Bm25,
        )
        .await
    }

    fn strategy(&self) -> RetrievalStrategy {
        RetrievalStrategy::Bm25
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FakeLexicalIndex {
        hits: Vec<(ChunkId, f32)>,
    }
    #[async_trait::async_trait]
    impl LexicalIndex for FakeLexicalIndex {
        async fn search(
            &self,
            _collection_id: &str,
            _query: &str,
            _top_k: usize,
        ) -> arcanum_core::Result<Vec<(ChunkId, f32)>> {
            Ok(self.hits.clone())
        }
    }

    fn record(text: &str, document_id: DocumentId) -> ChunkMetadataRecord {
        ChunkMetadataRecord {
            chunk_id: ChunkId::new(),
            document_id,
            collection_id: "col1".into(),
            version_num: 1,
            backend: ChunkBackend::Lexical,
            text: text.into(),
            chunk_index: 2,
            source_uri: "file://x.txt".into(),
            snapshot_uri: "file:///snap/x/1.raw".into(),
            canonical_uri: None,
            page: None,
            section: None,
            block_ids: vec![],
            offset_start: 10,
            offset_end: 10 + text.len(),
            ingested_at: chrono::Utc::now(),
        }
    }

    #[tokio::test]
    async fn test_bm25_retriever_uses_lexical_index_trait() {
        let rec = record("hello world", DocumentId::new());
        let store = Arc::new(InMemoryChunkMetadataStore::new());
        store.put(&rec).await.unwrap();
        let index: Arc<dyn LexicalIndex> = Arc::new(FakeLexicalIndex {
            hits: vec![(rec.chunk_id.clone(), 0.9)],
        });
        let retriever = Bm25Retriever::new(CollectionId("col1".into()), index, store);
        let query = Query::new("hello").with_collection(CollectionId("col1".into()));
        let result = retriever.retrieve(&query).await.unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].score, 0.9);
    }

    #[tokio::test]
    async fn bm25_returns_registry_text_and_document_id() {
        let doc_id = DocumentId::new();
        let rec = record("registry source text", doc_id.clone());
        let store = Arc::new(InMemoryChunkMetadataStore::new());
        store.put(&rec).await.unwrap();
        let index: Arc<dyn LexicalIndex> = Arc::new(FakeLexicalIndex {
            hits: vec![(rec.chunk_id.clone(), 1.5), (ChunkId::new(), 1.0)],
        });
        let retriever = Bm25Retriever::new_global(index, store);
        let query = Query::new("registry").with_collection(CollectionId("col1".into()));
        let result = retriever.retrieve(&query).await.unwrap();
        assert_eq!(result.len(), 1, "unresolved id must be skipped");
        let c = &result[0].indexed_chunk.chunk;
        assert_eq!(c.id, rec.chunk_id);
        assert_eq!(c.text, "registry source text");
        assert_eq!(c.document_id, doc_id);
        assert_eq!(
            (c.position.start, c.position.end, c.position.index),
            (10, 30, 2)
        );
    }
}
