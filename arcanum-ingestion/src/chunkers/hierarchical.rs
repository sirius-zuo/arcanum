use arcanum_core::{traits::Chunker, types::*, Result};
use async_trait::async_trait;

pub struct HierarchicalChunker;

impl HierarchicalChunker {
    pub fn new() -> Self { Self }
}

fn build_chunk(text: String, doc: &RawDocument, index: usize, title: String) -> Chunk {
    let mut metadata = std::collections::HashMap::new();
    if !title.is_empty() {
        metadata.insert("section_title".to_string(), serde_json::Value::String(title));
    }
    Chunk {
        id: ChunkId::new(),
        text,
        document_id: doc.id.clone(),
        collection_id: CollectionId("default".into()),
        position: ChunkPosition { start: 0, end: 0, index },
        metadata: ChunkMetadata(metadata),
    }
}

#[async_trait]
impl Chunker for HierarchicalChunker {
    async fn chunk(&self, doc: &RawDocument) -> Result<Vec<Chunk>> {
        let text = String::from_utf8_lossy(&doc.content).to_string();
        let mut sections: Vec<(String, String)> = Vec::new();
        let mut current_title = String::new();
        let mut current_body: Vec<String> = Vec::new();

        for line in text.lines() {
            if line.starts_with("### ") || line.starts_with("## ") || line.starts_with("# ") {
                if !current_body.is_empty() || !current_title.is_empty() {
                    sections.push((current_title.clone(), current_body.join("\n")));
                }
                current_title = line.trim_start_matches('#').trim().to_string();
                current_body = Vec::new();
            } else {
                current_body.push(line.to_string());
            }
        }
        if !current_body.is_empty() || !current_title.is_empty() {
            sections.push((current_title, current_body.join("\n")));
        }

        if sections.is_empty() {
            sections.push(("".to_string(), text));
        }

        let chunks = sections
            .into_iter()
            .enumerate()
            .filter_map(|(i, (title, body))| {
                let body_trimmed = body.trim().to_string();
                if body_trimmed.is_empty() && title.is_empty() {
                    None
                } else {
                    Some(build_chunk(body_trimmed, doc, i, title))
                }
            })
            .collect();
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
        let titles: Vec<&str> = chunks.iter().filter_map(|c| {
            c.metadata.0.get("section_title").and_then(|v| v.as_str())
        }).collect();
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
