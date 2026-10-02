use super::{line_spans, trimmed_span};
use arcanum_core::{traits::Chunker, types::*, Result};
use async_trait::async_trait;
use metrics;
use tracing::instrument;

pub struct HierarchicalChunker;

impl Default for HierarchicalChunker {
    fn default() -> Self {
        Self::new()
    }
}

impl HierarchicalChunker {
    pub fn new() -> Self {
        Self
    }
}

fn build_chunk(
    doc: &RawDocument,
    index: usize,
    title: String,
    source_text: &str,
    span: (usize, usize),
) -> Chunk {
    let mut metadata = std::collections::HashMap::new();
    if !title.is_empty() {
        metadata.insert(
            "section_title".to_string(),
            serde_json::Value::String(title),
        );
    }
    let (start, end) = trimmed_span(source_text, span.0, span.1).unwrap_or((span.0, span.0));
    Chunk {
        id: ChunkId::new(),
        text: source_text[start..end].to_string(),
        document_id: doc.id.clone(),
        collection_id: CollectionId("default".into()),
        position: ChunkPosition { start, end, index },
        metadata: ChunkMetadata(metadata),
        provenance: Default::default(),
    }
}

#[async_trait]
impl Chunker for HierarchicalChunker {
    #[instrument(skip(self, doc), fields(chunker = "hierarchical", input_len = doc.content.len(), chunk_count), err)]
    async fn chunk(&self, doc: &RawDocument) -> Result<Vec<Chunk>> {
        let text = String::from_utf8_lossy(&doc.content).to_string();
        // (title, body byte span). Spans come from line positions, so they stay
        // valid for any line ending.
        let mut sections: Vec<(String, (usize, usize))> = Vec::new();
        let mut current_title = String::new();
        let mut body: Option<(usize, usize)> = None;
        let mut next_body_start = 0usize;

        for (ls, le) in line_spans(&text) {
            let line = &text[ls..le];
            if line.starts_with("### ") || line.starts_with("## ") || line.starts_with("# ") {
                if body.is_some() || !current_title.is_empty() {
                    sections.push((
                        current_title.clone(),
                        body.unwrap_or((next_body_start, next_body_start)),
                    ));
                }
                current_title = line.trim_start_matches('#').trim().to_string();
                body = None;
                next_body_start = le;
            } else {
                body = Some(match body {
                    Some((s, _)) => (s, le),
                    None => (ls, le),
                });
            }
        }
        if body.is_some() || !current_title.is_empty() {
            sections.push((
                current_title,
                body.unwrap_or((next_body_start, next_body_start)),
            ));
        }

        if sections.is_empty() {
            sections.push(("".to_string(), (0, text.len())));
        }

        let mut chunks: Vec<Chunk> = Vec::new();
        for (i, (title, span)) in sections.into_iter().enumerate() {
            if text[span.0..span.1].trim().is_empty() && title.is_empty() {
                continue;
            }
            chunks.push(build_chunk(doc, i, title, &text, span));
        }
        tracing::Span::current().record("chunk_count", chunks.len());
        metrics::histogram!("arcanum_chunk_count", "chunker" => "hierarchical")
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
    async fn test_hierarchical_chunker_splits_at_headings() {
        let chunker = HierarchicalChunker::new();
        let md = "# Introduction\nSome intro text.\n## Background\nBackground text here.";
        let doc = make_doc(md);
        let chunks = chunker.chunk(&doc).await.unwrap();
        assert_eq!(chunks.len(), 2);
        let titles: Vec<&str> = chunks
            .iter()
            .filter_map(|c| c.metadata.0.get("section_title").and_then(|v| v.as_str()))
            .collect();
        assert!(titles.contains(&"Introduction"));
        assert!(titles.contains(&"Background"));
    }

    #[tokio::test]
    async fn test_hierarchical_chunker_plain_text_is_one_chunk() {
        let chunker = HierarchicalChunker::new();
        let doc = make_doc("No headings here, just plain text.");
        let chunks = chunker.chunk(&doc).await.unwrap();
        assert_eq!(chunks.len(), 1);
    }
}
