use arcanum_ingestion::{FileLoader, RawLoader, HttpLoader};
use arcanum_core::traits::{DocumentLoader, Source};
use std::io::Write;

#[tokio::test]
async fn test_file_loader_reads_markdown() {
    let mut tmp = tempfile::Builder::new().suffix(".md").tempfile().unwrap();
    tmp.write_all(b"# Hello\nWorld").unwrap();
    let loader = FileLoader::new();
    let source = Source::File(tmp.path().to_path_buf());
    assert!(loader.supports(&source));
    let doc = loader.load(&source).await.unwrap();
    assert_eq!(doc.mime_type, "text/markdown");
    assert!(!doc.content.is_empty());
}

#[tokio::test]
async fn test_raw_loader_passes_content_through() {
    let loader = RawLoader::new();
    let source = Source::Raw {
        content: b"hello world".to_vec(),
        mime_hint: Some("text/plain".into()),
        uri: "raw://test".into(),
    };
    assert!(loader.supports(&source));
    let doc = loader.load(&source).await.unwrap();
    assert_eq!(doc.content, b"hello world");
    assert_eq!(doc.mime_type, "text/plain");
    assert_eq!(doc.source_uri, "raw://test");
}

#[tokio::test]
async fn test_raw_loader_defaults_mime_when_hint_absent() {
    let loader = RawLoader::new();
    let source = Source::Raw {
        content: b"data".to_vec(),
        mime_hint: None,
        uri: "raw://x".into(),
    };
    let doc = loader.load(&source).await.unwrap();
    assert_eq!(doc.mime_type, "application/octet-stream");
}

#[test]
fn test_http_loader_supports_url() {
    let loader = HttpLoader::new();
    assert!(loader.supports(&Source::Url("https://example.com".into())));
    assert!(!loader.supports(&Source::File("/tmp/x".into())));
}

#[tokio::test]
async fn test_http_loader_returns_error_on_connection_refused() {
    let loader = HttpLoader::new();
    // Port 1 — always refused
    let source = Source::Url("http://127.0.0.1:1/doc".into());
    assert!(loader.load(&source).await.is_err());
}
