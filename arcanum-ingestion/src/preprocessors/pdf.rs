use arcanum_core::{traits::Preprocessor, types::*, Result, ArcanumError};
use async_trait::async_trait;

pub struct PdfParser;

impl PdfParser {
    pub fn new() -> Self { Self }
}

#[async_trait]
impl Preprocessor for PdfParser {
    async fn process(&self, mut doc: RawDocument) -> Result<RawDocument> {
        if doc.mime_type != "application/pdf" { return Ok(doc); }
        let text = pdf_extract::extract_text_from_mem(&doc.content)
            .map_err(|e| ArcanumError::Ingestion(format!("PDF parse error: {e}")))?;
        doc.content = text.into_bytes();
        doc.mime_type = "text/plain".to_string();
        Ok(doc)
    }
}
