use arcanum_core::{traits::*, types::*, Result};
use arcanum_vector::Bm25Index;
use async_trait::async_trait;
use std::sync::Arc;

pub struct Bm25Retriever {
    index: Arc<Bm25Index>,
}

impl Bm25Retriever {
    pub fn new(index: Arc<Bm25Index>) -> Self { Self { index } }
}

#[async_trait]
impl Retriever for Bm25Retriever {
    async fn retrieve(&self, query: &Query) -> Result<Vec<RetrievedChunk>> {
        let raw = self.index.search(&query.text, query.top_k)?;
        Ok(raw.into_iter().map(|(id, score)| RetrievedChunk {
            indexed_chunk: IndexedChunk {
                chunk: Chunk {
                    id: ChunkId::new(), text: id.clone(),
                    document_id: DocumentId::new(),
                    collection_id: CollectionId("default".into()),
                    position: ChunkPosition { start: 0, end: 0, index: 0 },
                    metadata: ChunkMetadata::default(),
                },
                vector: Vector(vec![]), token_vectors: None, store_id: id,
            },
            score, strategy: RetrievalStrategy::Bm25,
        }).collect())
    }

    fn strategy(&self) -> RetrievalStrategy { RetrievalStrategy::Bm25 }
}
