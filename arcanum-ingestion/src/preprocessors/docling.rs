use arcanum_core::{traits::Preprocessor, types::RawDocument, Result, ArcanumError};
use async_trait::async_trait;

pub enum DoclingBackend {
    Http {
        base_url: String,
        api_key: Option<String>,
        timeout_secs: u64,
        use_async: bool,
        poll_interval_ms: u64,
    },
    Cli {
        command: String,
    },
}

pub struct DoclingPreprocessor {
    backend: DoclingBackend,
    client: reqwest::Client,
}

impl DoclingPreprocessor {
    pub fn new(backend: DoclingBackend) -> Self {
        Self { backend, client: reqwest::Client::new() }
    }
}

const SUPPORTED_MIMES: &[&str] = &[
    "application/pdf",
    "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
    "application/vnd.openxmlformats-officedocument.presentationml.presentation",
    "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
    "application/epub+zip",
    "text/html",
    "application/xhtml+xml",
    "image/png",
    "image/jpeg",
    "image/tiff",
];

fn mime_to_ext(mime: &str) -> &'static str {
    match mime {
        "application/pdf" => "pdf",
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document" => "docx",
        "application/vnd.openxmlformats-officedocument.presentationml.presentation" => "pptx",
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet" => "xlsx",
        "application/epub+zip" => "epub",
        "text/html" | "application/xhtml+xml" => "html",
        "image/png" => "png",
        "image/jpeg" => "jpg",
        "image/tiff" => "tiff",
        _ => "bin",
    }
}

#[async_trait]
impl Preprocessor for DoclingPreprocessor {
    async fn process(&self, doc: RawDocument) -> Result<RawDocument> {
        if !SUPPORTED_MIMES.contains(&doc.mime_type.as_str()) {
            return Ok(doc);
        }
        match &self.backend {
            DoclingBackend::Http { base_url, api_key, timeout_secs, use_async, poll_interval_ms } => {
                self.convert_via_http(doc, base_url, api_key, *timeout_secs, *use_async, *poll_interval_ms).await
            }
            DoclingBackend::Cli { command } => {
                self.convert_via_cli(doc, command).await
            }
        }
    }
}

impl DoclingPreprocessor {
    async fn convert_via_http(
        &self, doc: RawDocument, _base_url: &str, _api_key: &Option<String>,
        _timeout_secs: u64, _use_async: bool, _poll_interval_ms: u64,
    ) -> Result<RawDocument> {
        Err(ArcanumError::Ingestion("DoclingPreprocessor HTTP backend not yet implemented".into()))
    }

    async fn convert_via_cli(&self, doc: RawDocument, _command: &str) -> Result<RawDocument> {
        Err(ArcanumError::Ingestion("DoclingPreprocessor CLI backend not yet implemented".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arcanum_core::types::DocumentId;

    fn raw_doc(content: Vec<u8>, mime_type: &str) -> RawDocument {
        RawDocument {
            id: DocumentId::new(),
            content,
            mime_type: mime_type.into(),
            source_uri: "test://doc.pdf".into(),
            metadata: Default::default(),
        }
    }

    #[tokio::test]
    async fn test_passthrough_unsupported_mime() {
        let p = DoclingPreprocessor::new(DoclingBackend::Http {
            base_url: "http://localhost:9999".into(),
            api_key: None,
            timeout_secs: 5,
            use_async: false,
            poll_interval_ms: 2000,
        });
        let doc = raw_doc(b"plain text".to_vec(), "text/plain");
        let out = p.process(doc).await.unwrap();
        assert_eq!(out.mime_type, "text/plain");
        assert_eq!(out.content, b"plain text");
    }

    #[tokio::test]
    async fn test_passthrough_octet_stream() {
        let p = DoclingPreprocessor::new(DoclingBackend::Cli {
            command: "false".into(),
        });
        let doc = raw_doc(b"binary".to_vec(), "application/octet-stream");
        let out = p.process(doc).await.unwrap();
        assert_eq!(out.mime_type, "application/octet-stream");
    }
}
