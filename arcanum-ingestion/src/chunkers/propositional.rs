use arcanum_core::{traits::Chunker, types::*, Result};
use async_trait::async_trait;

pub struct PropositionalChunker;
impl PropositionalChunker { pub fn new() -> Self { Self } }

#[async_trait]
impl Chunker for PropositionalChunker {
    async fn chunk(&self, doc: &RawDocument) -> Result<Vec<Chunk>> {
        let text = String::from_utf8_lossy(&doc.content);
        let props: Vec<&str> = text
            .split(|c| matches!(c, '.' | '!' | '?' | '\n'))
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .collect();
        Ok(props.into_iter().enumerate().map(|(i, p)| Chunk {
            id: ChunkId::new(), text: p.to_string(),
            document_id: doc.id.clone(),
            collection_id: CollectionId("default".into()),
            position: ChunkPosition { start: i, end: i + 1, index: i },
            metadata: ChunkMetadata::default(),
        }).collect())
    }
}
