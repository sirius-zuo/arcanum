use arcanum_core::types::RawDocument;
use std::collections::HashMap;
use scraper::{Html, Selector};
use tracing::instrument;

#[instrument(skip(doc))]
pub fn extract_title(doc: &RawDocument) -> HashMap<String, String> {
    let text = String::from_utf8_lossy(&doc.content);
    let mut meta = HashMap::new();
    if doc.mime_type == "text/html" || doc.mime_type == "application/xhtml+xml" {
        let parsed = Html::parse_document(&text);
        let sel = Selector::parse("title").unwrap();
        if let Some(el) = parsed.select(&sel).next() {
            let t = el.text().collect::<String>().trim().to_string();
            if !t.is_empty() { meta.insert("title".to_string(), t); }
        }
    } else {
        for line in text.lines() {
            if line.starts_with("# ") {
                let t = line.trim_start_matches('#').trim().to_string();
                if !t.is_empty() { meta.insert("title".to_string(), t); }
                break;
            }
        }
    }
    meta
}

#[cfg(test)]
mod tests {
    use super::*;
    use arcanum_core::types::DocumentId;

    fn make_doc(text: &str, mime: &str) -> RawDocument {
        RawDocument {
            id: DocumentId::new(),
            content: text.as_bytes().to_vec(),
            mime_type: mime.to_string(),
            source_uri: "test://x".to_string(),
            metadata: Default::default(),
        }
    }

    #[test]
    fn test_extract_title_from_markdown_h1() {
        let doc = make_doc("# My Document Title\n\nSome text.", "text/plain");
        let meta = extract_title(&doc);
        assert_eq!(meta.get("title").map(|s| s.as_str()), Some("My Document Title"));
    }

    #[test]
    fn test_extract_title_from_html_title_tag() {
        let doc = make_doc("<html><head><title>HTML Title</title></head></html>", "text/html");
        let meta = extract_title(&doc);
        assert_eq!(meta.get("title").map(|s| s.as_str()), Some("HTML Title"));
    }
}
