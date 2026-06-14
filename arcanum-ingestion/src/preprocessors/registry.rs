use arcanum_core::{traits::Preprocessor, types::RawDocument, Result};
use std::collections::HashMap;
use std::sync::Arc;

pub struct PreprocessorRegistry {
    chains: HashMap<String, Vec<Arc<dyn Preprocessor>>>,
}

impl PreprocessorRegistry {
    pub fn new() -> Self { Self { chains: HashMap::new() } }

    pub fn register(mut self, mime: &str, p: Arc<dyn Preprocessor>) -> Self {
        self.chains.entry(mime.to_string()).or_default().push(p);
        self
    }

    pub async fn process(&self, doc: RawDocument) -> Result<RawDocument> {
        let chain = self.chains.get(&doc.mime_type).cloned().unwrap_or_default();
        let mut out = doc;
        for p in chain {
            out = p.process(out).await?;
        }
        Ok(out)
    }

    pub fn docling_chains(preprocessor: Arc<dyn Preprocessor>) -> Self {
        Self::new()
            .register("application/pdf", preprocessor.clone())
            .register(
                "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
                preprocessor.clone(),
            )
            .register(
                "application/vnd.openxmlformats-officedocument.presentationml.presentation",
                preprocessor.clone(),
            )
            .register(
                "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
                preprocessor.clone(),
            )
            .register("application/epub+zip", preprocessor.clone())
            .register("text/html", preprocessor.clone())
            .register("application/xhtml+xml", preprocessor.clone())
            .register("image/png", preprocessor.clone())
            .register("image/jpeg", preprocessor.clone())
            .register("image/tiff", preprocessor.clone())
    }

    pub fn default_chains() -> Self {
        use crate::preprocessors::{HtmlCleaner, PdfParser, EpubParser, DocxPreprocessor};
        const DOCX: &str = "application/vnd.openxmlformats-officedocument.wordprocessingml.document";
        Self::new()
            .register("text/html",             Arc::new(HtmlCleaner::new()))
            .register("application/xhtml+xml", Arc::new(HtmlCleaner::new()))
            .register("application/pdf",       Arc::new(PdfParser::new()))
            .register("application/epub+zip",  Arc::new(EpubParser::new()))
            .register(DOCX,                    Arc::new(DocxPreprocessor::new()))
    }
}
