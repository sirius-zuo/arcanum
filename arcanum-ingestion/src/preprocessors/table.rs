use arcanum_core::{traits::Preprocessor, types::{DocumentId, RawDocument}, Result};
use async_trait::async_trait;
use scraper::{Html, Selector};
use tracing::instrument;

pub struct TableExtractor;

impl TableExtractor {
    pub fn new() -> Self { Self }
}

#[async_trait]
impl Preprocessor for TableExtractor {
    #[instrument(skip(self, doc), fields(preprocessor = "table", content_len = doc.content.len()), err)]
    async fn process(&self, mut doc: RawDocument) -> Result<RawDocument> {
        if doc.mime_type != "text/html" && doc.mime_type != "application/xhtml+xml" {
            return Ok(doc);
        }
        let html = String::from_utf8_lossy(&doc.content);
        let parsed = Html::parse_document(&html);
        let table_sel = Selector::parse("table").unwrap();
        let row_sel   = Selector::parse("tr").unwrap();
        let cell_sel  = Selector::parse("td, th").unwrap();

        let mut tables_text = Vec::new();
        for table in parsed.select(&table_sel) {
            let mut rows = Vec::new();
            for row in table.select(&row_sel) {
                let cells: Vec<String> = row.select(&cell_sel)
                    .map(|c| c.text().collect::<String>().trim().to_string())
                    .collect();
                if !cells.is_empty() {
                    rows.push(cells.join(" | "));
                }
            }
            if !rows.is_empty() {
                tables_text.push(rows.join("\n"));
            }
        }

        if !tables_text.is_empty() {
            let existing = String::from_utf8_lossy(&doc.content).to_string();
            let combined = format!("{}\n\n{}", tables_text.join("\n\n"), existing);
            doc.content = combined.into_bytes();
        }
        Ok(doc)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_table_extractor_converts_html_table() {
        let extractor = TableExtractor::new();
        let html = r#"<html><body><table><tr><th>Name</th><th>Score</th></tr><tr><td>Alice</td><td>95</td></tr></table></body></html>"#;
        let doc = RawDocument {
            id: DocumentId::new(),
            content: html.as_bytes().to_vec(),
            mime_type: "text/html".to_string(),
            source_uri: "test://x.html".to_string(),
            metadata: Default::default(),
        };
        let result = extractor.process(doc).await.unwrap();
        let text = String::from_utf8(result.content).unwrap();
        assert!(text.contains("Name | Score"));
        assert!(text.contains("Alice | 95"));
    }

    #[tokio::test]
    async fn test_table_extractor_passes_through_non_html() {
        let extractor = TableExtractor::new();
        let doc = RawDocument {
            id: DocumentId::new(),
            content: b"plain text".to_vec(),
            mime_type: "text/plain".to_string(),
            source_uri: "test://x".to_string(),
            metadata: Default::default(),
        };
        let result = extractor.process(doc).await.unwrap();
        assert_eq!(result.content, b"plain text");
    }
}
