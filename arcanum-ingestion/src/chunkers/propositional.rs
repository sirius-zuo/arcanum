use arcanum_core::{traits::Chunker, types::*, Result};
use async_trait::async_trait;
use metrics;
use tracing::instrument;

pub struct PropositionalChunker;
impl Default for PropositionalChunker {
    fn default() -> Self {
        Self::new()
    }
}

impl PropositionalChunker {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Chunker for PropositionalChunker {
    #[instrument(skip(self, doc), fields(chunker = "propositional", input_len = doc.content.len(), chunk_count), err)]
    async fn chunk(&self, doc: &RawDocument) -> Result<Vec<Chunk>> {
        let text = String::from_utf8_lossy(&doc.content);
        let mut chunks: Vec<Chunk> = Vec::new();
        let mut seg_start = 0usize;
        let ends = text
            .char_indices()
            .filter(|(_, c)| matches!(c, '.' | '!' | '?' | '\n'))
            .map(|(i, _)| i)
            .chain(std::iter::once(text.len()));
        for seg_end in ends {
            if let Some((s, e)) = super::trimmed_span(&text, seg_start, seg_end) {
                chunks.push(Chunk {
                    id: ChunkId::new(),
                    text: text[s..e].to_string(),
                    document_id: doc.id.clone(),
                    collection_id: CollectionId("default".into()),
                    position: ChunkPosition {
                        start: s,
                        end: e,
                        index: chunks.len(),
                    },
                    metadata: ChunkMetadata::default(),
                    provenance: Default::default(),
                });
            }
            // Every terminator is a single ASCII byte.
            seg_start = seg_end + 1;
        }
        tracing::Span::current().record("chunk_count", chunks.len());
        metrics::histogram!("arcanum_chunk_count", "chunker" => "propositional")
            .record(chunks.len() as f64);
        Ok(chunks)
    }
}
