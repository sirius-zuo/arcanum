use super::{line_spans, trimmed_span};
use arcanum_core::{traits::Chunker, types::*, Result};
use async_trait::async_trait;
use metrics;
use tracing::instrument;

pub struct StructureAwareChunker {
    max_chunk_chars: usize,
}

impl StructureAwareChunker {
    pub fn new(max_chunk_chars: usize) -> Self {
        Self { max_chunk_chars }
    }
}

fn build_chunk(doc: &RawDocument, index: usize, source_text: &str, span: (usize, usize)) -> Chunk {
    let (start, end) = trimmed_span(source_text, span.0, span.1).unwrap_or((span.0, span.0));
    Chunk {
        id: ChunkId::new(),
        text: source_text[start..end].to_string(),
        document_id: doc.id.clone(),
        collection_id: CollectionId("default".into()),
        position: ChunkPosition { start, end, index },
        metadata: ChunkMetadata::default(),
        provenance: Default::default(),
    }
}

/// Byte spans of blocks: runs of lines, with fenced code blocks kept separate.
fn split_into_blocks(text: &str) -> Vec<(usize, usize)> {
    let mut blocks = Vec::new();
    let mut in_code = false;
    let mut current: Option<(usize, usize)> = None;
    for (ls, le) in line_spans(text) {
        let extend = |c: Option<(usize, usize)>| Some((c.map_or(ls, |(s, _)| s), le));
        if text[ls..le].starts_with("```") {
            if in_code {
                blocks.push(extend(current).unwrap());
                current = None;
                in_code = false;
            } else {
                if let Some(c) = current.take() {
                    blocks.push(c);
                }
                current = extend(None);
                in_code = true;
            }
        } else {
            current = extend(current);
        }
    }
    if let Some(c) = current {
        blocks.push(c);
    }
    blocks
}

#[async_trait]
impl Chunker for StructureAwareChunker {
    #[instrument(skip(self, doc), fields(chunker = "structure", max_chunk_chars = self.max_chunk_chars, input_len = doc.content.len(), chunk_count), err)]
    async fn chunk(&self, doc: &RawDocument) -> Result<Vec<Chunk>> {
        let text = String::from_utf8_lossy(&doc.content).to_string();
        let blocks = split_into_blocks(&text);

        let mut chunks = Vec::new();
        // Source span of the prose accumulated so far.
        let mut current: Option<(usize, usize)> = None;
        let mut idx = 0;

        for (bs, be) in blocks {
            let block = &text[bs..be];
            let is_atomic = block.starts_with("```") || block.trim_start().starts_with('|');
            if is_atomic {
                if let Some(c) = current.take() {
                    if !text[c.0..c.1].trim().is_empty() {
                        chunks.push(build_chunk(doc, idx, &text, c));
                        idx += 1;
                    }
                }
                chunks.push(build_chunk(doc, idx, &text, (bs, be)));
                idx += 1;
            } else {
                // Accumulate prose lines up to max_chunk_chars
                for (ls, le) in line_spans(block) {
                    let (ls, le) = (bs + ls, bs + le);
                    if let Some(c) = current {
                        if c.1 > c.0 && (c.1 - c.0) + (le - ls) + 1 > self.max_chunk_chars {
                            chunks.push(build_chunk(doc, idx, &text, c));
                            idx += 1;
                            current = None;
                        }
                    }
                    current = Some((current.map_or(ls, |(s, _)| s), le));
                }
            }
        }
        if let Some(c) = current {
            if !text[c.0..c.1].trim().is_empty() {
                chunks.push(build_chunk(doc, idx, &text, c));
            }
        }
        if chunks.is_empty() {
            chunks.push(build_chunk(doc, 0, &text, (0, text.len())));
        }
        tracing::Span::current().record("chunk_count", chunks.len());
        metrics::histogram!("arcanum_chunk_count", "chunker" => "structure")
            .record(chunks.len() as f64);
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
        assert!(
            chunks.len() > 1,
            "should split long prose into multiple chunks"
        );
    }

    #[tokio::test]
    async fn test_short_text_single_chunk() {
        let chunker = StructureAwareChunker::new(1000);
        let doc = make_doc("Short text.");
        let chunks = chunker.chunk(&doc).await.unwrap();
        assert_eq!(chunks.len(), 1);
    }
}
