use arcanum_core::{traits::Preprocessor, types::*, Result};
use async_trait::async_trait;
use scraper::{Html, Selector};
use tracing::instrument;

pub struct HtmlCleaner;

impl HtmlCleaner {
    pub fn new() -> Self { Self }
}

/// Recursively collect text from an element, skipping script and style subtrees.
fn collect_text_recursive(el: scraper::ElementRef<'_>, parts: &mut Vec<String>) {
    for child in el.children() {
        if let Some(child_el) = scraper::ElementRef::wrap(child) {
            let name = child_el.value().name();
            if name == "script" || name == "style" {
                continue;
            }
            collect_text_recursive(child_el, parts);
        } else if let Some(text) = child.value().as_text() {
            let t = text.trim();
            if !t.is_empty() {
                parts.push(t.to_string());
            }
        }
    }
}

#[async_trait]
impl Preprocessor for HtmlCleaner {
    #[instrument(skip(self, doc), fields(preprocessor = "html", content_len = doc.content.len()), err)]
    async fn process(&self, mut doc: RawDocument) -> Result<RawDocument> {
        if doc.mime_type != "text/html" && doc.mime_type != "application/xhtml+xml" {
            return Ok(doc);
        }
        let html = String::from_utf8_lossy(&doc.content);
        let parsed = Html::parse_document(&html);
        let mut text_parts: Vec<String> = Vec::new();
        let body_sel = Selector::parse("body").unwrap();
        if let Some(body) = parsed.select(&body_sel).next() {
            collect_text_recursive(body, &mut text_parts);
        } else {
            collect_text_recursive(parsed.root_element(), &mut text_parts);
        }
        let cleaned = text_parts.join(" ").split_whitespace().collect::<Vec<_>>().join(" ");
        doc.content = cleaned.into_bytes();
        doc.mime_type = "text/plain".to_string();
        Ok(doc)
    }

    fn canonical(&self, _doc_id: &DocumentId) -> Option<serde_json::Value> {
        None
    }

    fn set_canonical(&self, _doc_id: &DocumentId, _canonical: serde_json::Value) {
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_html_doc(html: &str) -> RawDocument {
        RawDocument {
            id: DocumentId::new(),
            content: html.as_bytes().to_vec(),
            mime_type: "text/html".to_string(),
            source_uri: "test://x.html".to_string(),
            metadata: Default::default(),
        }
    }

    #[tokio::test]
    async fn test_html_cleaner_basic() {
        let cleaner = HtmlCleaner::new();
        let doc = make_html_doc("<html><body><p>Hello world</p></body></html>");
        let result = cleaner.process(doc).await.unwrap();
        let text = String::from_utf8(result.content).unwrap();
        assert!(text.contains("Hello world"));
        assert_eq!(result.mime_type, "text/plain");
    }

    #[tokio::test]
    async fn test_html_cleaner_removes_script_content() {
        let cleaner = HtmlCleaner::new();
        let doc = make_html_doc(r#"<html><body><p>Hello world</p><script>alert('injection');</script><style>.foo { color: red; }</style><p>Goodbye</p></body></html>"#);
        let result = cleaner.process(doc).await.unwrap();
        let text = String::from_utf8(result.content).unwrap();
        assert!(text.contains("Hello world"));
        assert!(text.contains("Goodbye"));
        assert!(!text.contains("alert"), "script content must be removed");
        assert!(!text.contains(".foo"), "style content must be removed");
    }

    #[tokio::test]
    async fn test_html_cleaner_passthrough_non_html() {
        let cleaner = HtmlCleaner::new();
        let doc = RawDocument {
            id: DocumentId::new(),
            content: b"plain text".to_vec(),
            mime_type: "text/plain".to_string(),
            source_uri: "test://x".to_string(),
            metadata: Default::default(),
        };
        let result = cleaner.process(doc).await.unwrap();
        assert_eq!(result.mime_type, "text/plain");
        assert_eq!(result.content, b"plain text");
    }
}
