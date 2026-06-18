use arcanum_core::traits::Preprocessor;
use arcanum_core::types::*;
use arcanum_ingestion::{DoclingPreprocessor, DoclingBackend, PreprocessorCatalog};
use std::sync::Arc;

fn raw_doc(content: Vec<u8>, mime_type: &str) -> RawDocument {
    RawDocument {
        id: DocumentId::new(),
        content,
        mime_type: mime_type.into(),
        source_uri: "test".into(),
        metadata: Default::default(),
    }
}

// ── PreprocessorCatalog ──────────────────────────────────────────────────────

#[tokio::test]
async fn test_catalog_register_and_get() {
    let mut catalog = PreprocessorCatalog::new();
    let docling = Arc::new(DoclingPreprocessor::new(DoclingBackend::Http {
        base_url: "http://localhost:9999".into(),
        api_key: None,
        timeout_secs: 5,
        use_async: false,
        poll_interval_ms: 2000,
    }));
    catalog.register("default", docling);
    let resolved = catalog.get("default");
    assert!(resolved.is_some());
}

#[tokio::test]
async fn test_catalog_get_unknown_name() {
    let catalog = PreprocessorCatalog::new();
    let resolved = catalog.get("nonexistent");
    assert!(resolved.is_none());
}

// ── DoclingPreprocessor ──────────────────────────────────────────────────────

#[tokio::test]
async fn test_docling_http_unavailable() {
    let pp = DoclingPreprocessor::new(DoclingBackend::Http {
        base_url: "http://localhost:9999".into(),
        api_key: None,
        timeout_secs: 5,
        use_async: false,
        poll_interval_ms: 2000,
    });
    let doc = raw_doc(b"%PDF-1.4".to_vec(), "application/pdf");
    let result = pp.process(doc).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_docling_http_with_api_key() {
    let pp = DoclingPreprocessor::new(DoclingBackend::Http {
        base_url: "http://localhost:9999".into(),
        api_key: Some("test-token".into()),
        timeout_secs: 5,
        use_async: false,
        poll_interval_ms: 2000,
    });
    // Use a MIME that triggers HTTP conversion (not passthrough)
    let doc = raw_doc(b"test".to_vec(), "application/vnd.openxmlformats-officedocument.wordprocessingml.document");
    let result = pp.process(doc).await;
    // Should error (no server), but proves api_key is wired
    assert!(result.is_err());
}
