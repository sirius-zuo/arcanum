use arcanum_core::{traits::Chunker, types::*, Result};
use async_trait::async_trait;
use tracing::instrument;

pub struct FixedSizeChunker { chunk_size: usize, overlap: usize }

impl FixedSizeChunker {
    pub fn new(chunk_size: usize, overlap: usize) -> Self {
        assert!(overlap < chunk_size);
        Self { chunk_size, overlap }
    }
}

#[async_trait]
impl Chunker for FixedSizeChunker {
    #[instrument(skip(self, doc), fields(chunker = "fixed_size", chunk_size = self.chunk_size, overlap = self.overlap, input_len = doc.content.len(), chunk_count), err)]
    async fn chunk(&self, doc: &RawDocument) -> Result<Vec<Chunk>> {
        let text = String::from_utf8_lossy(&doc.content);
        if text.trim().is_empty() { return Ok(vec![]); }
        let chars: Vec<char> = text.chars().collect();
        let step = self.chunk_size - self.overlap;
        let mut chunks = vec![];
        let mut start = 0usize;
        let mut index = 0usize;
        while start < chars.len() {
            let end = (start + self.chunk_size).min(chars.len());
            let chunk_text: String = chars[start..end].iter().collect();
            let trimmed = chunk_text.trim().to_string();
            if !trimmed.is_empty() {
                chunks.push(Chunk {
                    id: ChunkId::new(), text: trimmed,
                    document_id: doc.id.clone(),
                    collection_id: CollectionId("default".into()),
                    position: ChunkPosition { start, end, index },
                    metadata: ChunkMetadata::default(),
                });
                index += 1;
            }
            if end == chars.len() { break; }
            start += step;
        }
        tracing::Span::current().record("chunk_count", chunks.len());
        Ok(chunks)
    }
}
