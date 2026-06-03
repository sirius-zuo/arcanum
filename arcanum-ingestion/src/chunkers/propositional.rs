use arcanum_core::{traits::Chunker, types::*, Result};
use async_trait::async_trait;
use tracing::instrument;
use metrics;

pub struct PropositionalChunker;
impl PropositionalChunker { pub fn new() -> Self { Self } }

#[async_trait]
impl Chunker for PropositionalChunker {
    #[instrument(skip(self, doc), fields(chunker = "propositional", input_len = doc.content.len(), chunk_count), err)]
    async fn chunk(&self, doc: &RawDocument) -> Result<Vec<Chunk>> {
        let text = String::from_utf8_lossy(&doc.content);
        let props: Vec<&str> = text
            .split(|c| matches!(c, '.' | '!' | '?' | '\n'))
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .collect();
        let chunks: Vec<Chunk> = props.into_iter().enumerate().map(|(i, p)| Chunk {
            id: ChunkId::new(), text: p.to_string(),
            document_id: doc.id.clone(),
            collection_id: CollectionId("default".into()),
            position: ChunkPosition { start: i, end: i + 1, index: i },
            metadata: ChunkMetadata::default(),
        }).collect();
        tracing::Span::current().record("chunk_count", chunks.len());
        metrics::histogram!("arcanum_chunk_count", "chunker" => "propositional").record(chunks.len() as f64);
        Ok(chunks)
    }
}
