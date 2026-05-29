use arcanum_core::{traits::*, types::*, Result, ArcanumError};
use arcanum_vector::Bm25Index;
use async_trait::async_trait;
use std::sync::Arc;

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
    collection_id: CollectionId,
    index: Arc<Bm25Index>,
}

impl Bm25Retriever {
    pub fn new(collection_id: CollectionId, index: Arc<Bm25Index>) -> Self {
        Self { collection_id, index }
    }
}

#[async_trait]
impl Retriever for Bm25Retriever {
    async fn retrieve(&self, query: &Query) -> Result<Vec<RetrievedChunk>> {
        // Deny requests that target a different or unspecified collection.
        match &query.collection_id {
            None => return Err(ArcanumError::Config(
                "Bm25Retriever requires an explicit collection_id".into()
            )),
            Some(cid) if cid.0 != self.collection_id.0 => {
                return Err(ArcanumError::Config(format!(
                    "Bm25Retriever for '{}' cannot serve collection '{}'",
                    self.collection_id.0, cid.0
                )));
            }
            _ => {}
        }

        let raw = self.index.search(&query.text, query.top_k)?;
        let collection_id = self.collection_id.clone();
        Ok(raw.into_iter().map(|(store_id, score)| RetrievedChunk {
            indexed_chunk: IndexedChunk {
                chunk: Chunk {
                    // ChunkId and DocumentId are stubs — real values require
                    // metadata lookup by store_id (see KNOWN LIMITATION above).
                    id: ChunkId::new(),
                    text: store_id.clone(),
                    document_id: DocumentId::new(),
                    collection_id: collection_id.clone(),
                    position: ChunkPosition { start: 0, end: 0, index: 0 },
                    metadata: ChunkMetadata::default(),
                },
                vector: Vector(vec![]), token_vectors: None, store_id,
            },
            score, strategy: RetrievalStrategy::Bm25,
        }).collect())
    }

    fn strategy(&self) -> RetrievalStrategy { RetrievalStrategy::Bm25 }
}
