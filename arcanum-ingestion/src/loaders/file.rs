use arcanum_core::{traits::{DocumentLoader, Source}, types::*, Result, ArcanumError};
use async_trait::async_trait;

pub struct FileLoader;

impl FileLoader {
    pub fn new() -> Self { Self }

    fn detect_mime(path: &std::path::Path) -> &'static str {
        match path.extension().and_then(|e| e.to_str()) {
            Some("md") | Some("markdown") => "text/markdown",
            Some("html") | Some("htm")    => "text/html",
            Some("txt")                   => "text/plain",
            Some("pdf")                   => "application/pdf",
            Some("epub")                  => "application/epub+zip",
            Some("docx")                  => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
            _                             => "application/octet-stream",
        }
    }
}

#[async_trait]
impl DocumentLoader for FileLoader {
    async fn load(&self, source: &Source) -> Result<RawDocument> {
        let Source::File(path) = source else {
            return Err(ArcanumError::Ingestion("FileLoader only handles Source::File".into()));
        };
        let content = tokio::fs::read(path).await
            .map_err(|e| ArcanumError::Ingestion(e.to_string()))?;
        Ok(RawDocument {
            id: DocumentId::new(),
            mime_type: Self::detect_mime(path).to_string(),
            source_uri: path.to_string_lossy().to_string(),
            content,
            metadata: Default::default(),
        })
    }

    fn supports(&self, source: &Source) -> bool {
        matches!(source, Source::File(_))
    }
}
