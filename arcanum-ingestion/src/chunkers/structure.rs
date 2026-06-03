use arcanum_core::{traits::Chunker, types::*, Result};
use async_trait::async_trait;
use tracing::instrument;

pub struct StructureAwareChunker {
    max_chunk_chars: usize,
}

impl StructureAwareChunker {
    pub fn new(max_chunk_chars: usize) -> Self { Self { max_chunk_chars } }
}

fn build_chunk(text: String, doc: &RawDocument, index: usize, source_text: &str) -> Chunk {
    let t = text.trim().to_string();
    let start = source_text.find(&t).unwrap_or(0);
    let end = start + t.len();
    Chunk {
        id: ChunkId::new(),
        text: t,
        document_id: doc.id.clone(),
        collection_id: CollectionId("default".into()),
        position: ChunkPosition { start, end, index },
        metadata: ChunkMetadata::default(),
    }
}

fn split_into_blocks(text: &str) -> Vec<String> {
    let mut blocks = Vec::new();
    let mut in_code = false;
    let mut current: Vec<String> = Vec::new();
    for line in text.lines() {
        if line.starts_with("```") {
            if in_code {
                current.push(line.to_string());
                blocks.push(current.join("\n"));
                current = Vec::new();
                in_code = false;
            } else {
                if !current.is_empty() {
                    blocks.push(current.join("\n"));
                    current = Vec::new();
                }
                current.push(line.to_string());
                in_code = true;
            }
        } else {
            current.push(line.to_string());
        }
    }
    if !current.is_empty() { blocks.push(current.join("\n")); }
    blocks
}

#[async_trait]
impl Chunker for StructureAwareChunker {
    #[instrument(skip(self, doc), fields(chunker = "structure", max_chunk_chars = self.max_chunk_chars, input_len = doc.content.len(), chunk_count), err)]
    async fn chunk(&self, doc: &RawDocument) -> Result<Vec<Chunk>> {
        let text = String::from_utf8_lossy(&doc.content).to_string();
        let blocks = split_into_blocks(&text);

        let mut chunks = Vec::new();
        let mut current = String::new();
        let mut idx = 0;

        for block in blocks {
            let is_atomic = block.starts_with("```") || block.trim_start().starts_with('|');
            if is_atomic {
                if !current.trim().is_empty() {
                    chunks.push(build_chunk(current.trim().to_string(), doc, idx, &text));
                    idx += 1;
                    current = String::new();
                }
                chunks.push(build_chunk(block.trim().to_string(), doc, idx, &text));
                idx += 1;
            } else {
                // Split prose block into lines and accumulate up to max_chunk_chars
                for line in block.lines() {
                    if !current.is_empty() && current.len() + line.len() + 1 > self.max_chunk_chars {
                        chunks.push(build_chunk(current.trim().to_string(), doc, idx, &text));
                        idx += 1;
                        current = String::new();
                    }
                    if !current.is_empty() { current.push('\n'); }
                    current.push_str(line);
                }
            }
        }
        if !current.trim().is_empty() {
            chunks.push(build_chunk(current.trim().to_string(), doc, idx, &text));
        }
        if chunks.is_empty() {
            chunks.push(build_chunk(text.trim().to_string(), doc, 0, &text));
        }
        tracing::Span::current().record("chunk_count", chunks.len());
        Ok(chunks)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_doc(text: &str) -> RawDocument {
        RawDocument {
            id: DocumentId::new(),
            content: text.as_bytes().to_vec(),
            mime_type: "text/plain".to_string(),
            source_uri: "test://x.md".to_string(),
            metadata: Default::default(),
        }
    }

    #[tokio::test]
    async fn test_code_block_stays_atomic() {
        let chunker = StructureAwareChunker::new(50);
        let text = "Some prose before the code.\n```\nfn hello() {\n    println!(\"Hello\");\n}\n```\nSome prose after.";
        let doc = make_doc(text);
        let chunks = chunker.chunk(&doc).await.unwrap();
        // Find the chunk containing the code block
        let code_chunk = chunks.iter().find(|c| c.text.contains("fn hello()"));
        assert!(code_chunk.is_some(), "code block chunk should exist");
        // The code block should not be split — it should all be in one chunk
        let code_text = &code_chunk.unwrap().text;
        assert!(code_text.contains("```"), "code fences must be preserved");
        assert!(code_text.contains("fn hello()"));
        assert!(code_text.contains("println!"));
    }

    #[tokio::test]
    async fn test_prose_split_at_max_chars() {
        let chunker = StructureAwareChunker::new(20);
        let text = "First paragraph of text.\nSecond paragraph of text.";
        let doc = make_doc(text);
        let chunks = chunker.chunk(&doc).await.unwrap();
        assert!(chunks.len() > 1, "should split long prose into multiple chunks");
    }

    #[tokio::test]
    async fn test_short_text_single_chunk() {
        let chunker = StructureAwareChunker::new(1000);
        let doc = make_doc("Short text.");
        let chunks = chunker.chunk(&doc).await.unwrap();
        assert_eq!(chunks.len(), 1);
    }
}
