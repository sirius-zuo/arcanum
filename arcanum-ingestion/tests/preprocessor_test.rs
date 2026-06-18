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

// Catalog-dispatch round trip: registers a DoclingPreprocessor under "default"
// and verifies a document processed through the resolved catalog entry — not
// DoclingPreprocessor directly — comes back as parsed markdown. Docling's own
// HTTP request/response details (headers, multipart shape, etc.) are covered
// by DoclingPreprocessor's internal tests in docling.rs; this test exists to
// prove the catalog's resolved Arc<dyn Preprocessor> actually does the work.
#[tokio::test]
async fn test_catalog_dispatched_docling_converts_pdf_to_markdown() {
    use wiremock::{MockServer, Mock, ResponseTemplate};
    use wiremock::matchers::{method, path};

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/convert/file"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "document": { "md_content": "# PDF content" },
            "status": "success"
        })))
        .mount(&server)
        .await;

    let mut catalog = PreprocessorCatalog::new();
    catalog.register("default", Arc::new(DoclingPreprocessor::new(DoclingBackend::Http {
        base_url: server.uri(),
        api_key: None,
        timeout_secs: 10,
        use_async: false,
        poll_interval_ms: 2000,
    })));

    let pp = catalog.get("default").expect("default preprocessor should be registered");
    let doc = raw_doc(b"%PDF-1.4".to_vec(), "application/pdf");
    let out = pp.process(doc).await.unwrap();
    assert_eq!(out.mime_type, "text/markdown");
    assert!(String::from_utf8(out.content).unwrap().contains("PDF content"));
}
