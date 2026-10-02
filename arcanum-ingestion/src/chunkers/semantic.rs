use arcanum_core::{traits::Chunker, types::*, Result};
use async_trait::async_trait;
use metrics;
use tracing::instrument;

pub struct SemanticChunker {
    max_chars: usize,
}

impl SemanticChunker {
    pub fn new(max_chars: usize) -> Self {
        Self { max_chars }
    }
}

#[async_trait]
impl Chunker for SemanticChunker {
    #[instrument(skip(self, doc), fields(chunker = "semantic", max_chars = self.max_chars, input_len = doc.content.len(), chunk_count), err)]
    async fn chunk(&self, doc: &RawDocument) -> Result<Vec<Chunk>> {
        let text = String::from_utf8_lossy(&doc.content);
        let mut chunks = vec![];
        // Byte range of the sentences accumulated so far.
        let mut cur_start = 0usize;
        let mut cur_end = 0usize;
        let mut index = 0usize;
        let mut flush = |cur_start: usize, cur_end: usize, chunks: &mut Vec<Chunk>| {
            if let Some((s, e)) = super::trimmed_span(&text, cur_start, cur_end) {
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
        };
        for sentence in text.split_inclusive(['.', '!', '?']) {
            let sentence_end = cur_end + sentence.len();
            if cur_end - cur_start + sentence.len() > self.max_chars && cur_end > cur_start {
                flush(cur_start, cur_end, &mut chunks);
                cur_start = cur_end;
            }
            cur_end = sentence_end;
        }
        flush(cur_start, cur_end, &mut chunks);
        tracing::Span::current().record("chunk_count", chunks.len());
        metrics::histogram!("arcanum_chunk_count", "chunker" => "semantic")
            .record(chunks.len() as f64);
        Ok(chunks)
    }
}
