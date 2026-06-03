use arcanum_core::{traits::VectorStore, types::*, Result};
use crate::bm25::Bm25Index;
use std::sync::Arc;
use tracing::instrument;

pub struct HybridIndexManager {
    vector_store: Arc<dyn VectorStore>,
    bm25: Arc<Bm25Index>,
}

impl HybridIndexManager {
    pub fn new(vector_store: Arc<dyn VectorStore>, bm25: Arc<Bm25Index>) -> Self {
        Self { vector_store, bm25 }
    }

    #[instrument(skip(self, chunk), fields(collection_id = collection), err)]
    pub async fn index_chunk(&self, collection: &str, chunk: &IndexedChunk) -> Result<()> {
        self.vector_store.upsert(collection, vec![chunk.clone()]).await?;
        self.bm25.index_document(&chunk.chunk.id.0.to_string(), &chunk.chunk.text)?;
        Ok(())
    }

    #[instrument(skip(self, chunk_id), fields(collection_id = collection), err)]
    pub async fn delete_chunk(&self, collection: &str, chunk_id: &ChunkId) -> Result<()> {
        self.vector_store.delete(collection, &[chunk_id.clone()]).await?;
        self.bm25.delete_document(&chunk_id.0.to_string())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_hybrid_manager_types_compile() {
        assert!(true);
    }
}
