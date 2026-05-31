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

use arcanum_core::traits::{CloudProvider, ConnectorKind};
use arcanum_ingestion::loaders::{DatabaseLoader, CloudStorageLoader, GitLoader, ConnectorLoader};

#[test]
fn test_stub_loaders_support_correct_variants() {
    assert!(DatabaseLoader::new().supports(&Source::Database {
        connection_string: "postgres://localhost/db".into(),
        query: "SELECT 1".into(),
        display_uri: "postgres://localhost/db".into(),
    }));
    assert!(CloudStorageLoader::new().supports(&Source::CloudStorage {
        provider: CloudProvider::S3,
        bucket: "b".into(),
        key: "k".into(),
    }));
    assert!(GitLoader::new().supports(&Source::Git {
        repo_url: "https://github.com/x/y".into(),
        branch: "main".into(),
        path_glob: None,
    }));
    assert!(ConnectorLoader::new().supports(&Source::Connector {
        provider: ConnectorKind::Notion,
        resource_id: "page-123".into(),
    }));
}

#[tokio::test]
async fn test_stub_loaders_return_error_on_load() {
    let src = Source::Database {
        connection_string: "postgres://localhost/db".into(),
        query: "SELECT 1".into(),
        display_uri: "postgres://localhost/db".into(),
    };
    assert!(DatabaseLoader::new().load(&src).await.is_err());
}
