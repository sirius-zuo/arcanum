use arcanum_core::{traits::Chunker, types::*, Result};
use async_trait::async_trait;
use metrics;
use tracing::instrument;

pub struct FixedSizeChunker {
    chunk_size: usize,
    overlap: usize,
}

impl FixedSizeChunker {
    pub fn new(chunk_size: usize, overlap: usize) -> Self {
        assert!(overlap < chunk_size);
        Self {
            chunk_size,
            overlap,
        }
    }
}

#[async_trait]
impl Chunker for FixedSizeChunker {
    #[instrument(skip(self, doc), fields(chunker = "fixed_size", chunk_size = self.chunk_size, overlap = self.overlap, input_len = doc.content.len(), chunk_count), err)]
    async fn chunk(&self, doc: &RawDocument) -> Result<Vec<Chunk>> {
        let text = String::from_utf8_lossy(&doc.content);
        if text.trim().is_empty() {
            return Ok(vec![]);
        }
        // Byte offset of every char boundary, plus the end of the text, so that
        // windows measured in chars map to byte ranges that slice cleanly.
        let mut bounds: Vec<usize> = text.char_indices().map(|(i, _)| i).collect();
        let n_chars = bounds.len();
        bounds.push(text.len());
        let step = self.chunk_size - self.overlap;
        let mut chunks = vec![];
        let mut start = 0usize;
        let mut index = 0usize;
        while start < n_chars {
            let end = (start + self.chunk_size).min(n_chars);
            if let Some((s, e)) = super::trimmed_span(&text, bounds[start], bounds[end]) {
                chunks.push(Chunk {
                    id: ChunkId::new(),
                    text: text[s..e].to_string(),
                    document_id: doc.id.clone(),
                    collection_id: CollectionId("default".into()),
                    position: ChunkPosition {
                        start: s,
                        end: e,
                        index,
                    },
                    metadata: ChunkMetadata::default(),
                    provenance: Default::default(),
                });
                index += 1;
            }
            if end == n_chars {
                break;
            }
            start += step;
        }
        tracing::Span::current().record("chunk_count", chunks.len());
        metrics::histogram!("arcanum_chunk_count", "chunker" => "fixed")
            .record(chunks.len() as f64);
        Ok(chunks)
    }
}
