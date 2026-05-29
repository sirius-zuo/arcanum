use arcanum_core::{traits::Preprocessor, types::*, Result};
use async_trait::async_trait;
use scraper::Html;

pub struct HtmlCleaner;

impl HtmlCleaner {
    pub fn new() -> Self { Self }
}

#[async_trait]
impl Preprocessor for HtmlCleaner {
    async fn process(&self, mut doc: RawDocument) -> Result<RawDocument> {
        if doc.mime_type != "text/html" { return Ok(doc); }
        let html = String::from_utf8_lossy(&doc.content);
        let parsed = Html::parse_document(&html);
        let text: String = parsed.root_element().text()
            .collect::<Vec<_>>()
            .join(" ");
        let cleaned = text.split_whitespace().collect::<Vec<_>>().join(" ");
        doc.content = cleaned.into_bytes();
        doc.mime_type = "text/plain".to_string();
        Ok(doc)
    }
}
