use arcanum_core::{traits::Preprocessor, types::*, Result, ArcanumError};
use async_trait::async_trait;
use std::io::Read;
use tracing::instrument;

pub struct DocxPreprocessor;

impl DocxPreprocessor {
    pub fn new() -> Self { Self }
}

const DOCX_MIME: &str = "application/vnd.openxmlformats-officedocument.wordprocessingml.document";

#[async_trait]
impl Preprocessor for DocxPreprocessor {
    #[instrument(skip(self, doc), fields(preprocessor = "docx", content_len = doc.content.len()), err)]
    async fn process(&self, mut doc: RawDocument) -> Result<RawDocument> {
        if doc.mime_type != DOCX_MIME {
            return Ok(doc);
        }
        let cursor = std::io::Cursor::new(doc.content.clone());
        let mut archive = zip::ZipArchive::new(cursor)
            .map_err(|e| ArcanumError::Ingestion(format!("DOCX open error: {}", e)))?;
        let mut xml_file = archive.by_name("word/document.xml")
            .map_err(|e| ArcanumError::Ingestion(format!("DOCX missing document.xml: {}", e)))?;
        let mut xml_content = String::new();
        xml_file.read_to_string(&mut xml_content)
            .map_err(|e| ArcanumError::Ingestion(format!("DOCX read error: {}", e)))?;
        let xml = roxmltree::Document::parse(&xml_content)
            .map_err(|e| ArcanumError::Ingestion(format!("DOCX XML parse error: {}", e)))?;
        let text: String = xml.descendants()
            .filter(|n| n.is_text())
            .filter(|n| n.parent().map(|p| p.tag_name().name() == "t").unwrap_or(false))
            .map(|n| n.text().unwrap_or(""))
            .collect::<Vec<_>>()
            .join(" ");
        let cleaned = text.split_whitespace().collect::<Vec<_>>().join(" ");
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

    #[tokio::test]
    async fn test_docx_preprocessor_passes_through_non_docx() {
        let proc = DocxPreprocessor::new();
        let doc = RawDocument {
            id: DocumentId::new(),
            content: b"hello".to_vec(),
            mime_type: "text/plain".to_string(),
            source_uri: "test://x".to_string(),
            metadata: Default::default(),
        };
        let result = proc.process(doc).await.unwrap();
        assert_eq!(result.mime_type, "text/plain");
        assert_eq!(result.content, b"hello");
    }

    #[tokio::test]
    async fn test_docx_preprocessor_rejects_invalid_docx() {
        let proc = DocxPreprocessor::new();
        let doc = RawDocument {
            id: DocumentId::new(),
            content: b"not a zip".to_vec(),
            mime_type: DOCX_MIME.to_string(),
            source_uri: "test://x.docx".to_string(),
            metadata: Default::default(),
        };
        let result = proc.process(doc).await;
        assert!(result.is_err());
    }
}
