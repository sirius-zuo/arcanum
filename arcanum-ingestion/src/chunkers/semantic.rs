use arcanum_core::{traits::Chunker, types::*, Result};
use async_trait::async_trait;

pub struct SemanticChunker { max_chars: usize }

impl SemanticChunker { pub fn new(max_chars: usize) -> Self { Self { max_chars } } }

#[async_trait]
impl Chunker for SemanticChunker {
    async fn chunk(&self, doc: &RawDocument) -> Result<Vec<Chunk>> {
        let text = String::from_utf8_lossy(&doc.content);
        let sentences: Vec<&str> = text.split_inclusive(|c| matches!(c, '.' | '!' | '?')).collect();
        let mut chunks = vec![];
        let mut current = String::new();
        let mut start = 0usize;
        let mut index = 0usize;
        for sentence in sentences {
            if current.len() + sentence.len() > self.max_chars && !current.is_empty() {
                let trimmed = current.trim().to_string();
                if !trimmed.is_empty() {
                    chunks.push(Chunk {
                        id: ChunkId::new(), text: trimmed.clone(),
                        document_id: doc.id.clone(),
                        collection_id: CollectionId("default".into()),
                        position: ChunkPosition { start, end: start + trimmed.len(), index },
                        metadata: ChunkMetadata::default(),
                    });
                    index += 1;
                }
                start += current.len();
                current = sentence.to_string();
            } else {
                current.push_str(sentence);
            }
        }
        if !current.trim().is_empty() {
            chunks.push(Chunk {
                id: ChunkId::new(), text: current.trim().to_string(),
                document_id: doc.id.clone(),
                collection_id: CollectionId("default".into()),
                position: ChunkPosition { start, end: start + current.len(), index },
                metadata: ChunkMetadata::default(),
            });
        }
        Ok(chunks)
    }
}
