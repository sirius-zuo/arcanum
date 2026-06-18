use arcanum_core::traits::Preprocessor;
use arcanum_core::types::RawDocument;
use arcanum_ingestion::PreprocessorCatalog;
use std::sync::Arc;

struct StubPreprocessor;
#[async_trait::async_trait]
impl Preprocessor for StubPreprocessor {
    async fn process(&self, doc: RawDocument) -> arcanum_core::Result<RawDocument> {
        Ok(doc)
    }
}

#[test]
fn get_returns_none_for_unregistered_name() {
    let catalog = PreprocessorCatalog::new();
    assert!(catalog.get("default").is_none());
}

#[test]
fn get_returns_registered_preprocessor() {
    let mut catalog = PreprocessorCatalog::new();
    catalog.register("default", Arc::new(StubPreprocessor));
    assert!(catalog.get("default").is_some());
    assert!(catalog.get("other").is_none());
}

#[tokio::test]
async fn register_overwrites_existing_name() {
    struct FirstPreprocessor;
    #[async_trait::async_trait]
    impl Preprocessor for FirstPreprocessor {
        async fn process(&self, mut doc: RawDocument) -> arcanum_core::Result<RawDocument> {
            doc.mime_type = "first".into();
            Ok(doc)
        }
    }
    struct SecondPreprocessor;
    #[async_trait::async_trait]
    impl Preprocessor for SecondPreprocessor {
        async fn process(&self, mut doc: RawDocument) -> arcanum_core::Result<RawDocument> {
            doc.mime_type = "second".into();
            Ok(doc)
        }
    }

    let mut catalog = PreprocessorCatalog::new();
    catalog.register("default", Arc::new(FirstPreprocessor));
    catalog.register("default", Arc::new(SecondPreprocessor));

    let doc = RawDocument {
        id: arcanum_core::types::DocumentId::new(),
        content: b"x".to_vec(),
        mime_type: "text/plain".into(),
        source_uri: "test".into(),
        metadata: Default::default(),
    };
    let result = catalog.get("default").unwrap().process(doc).await.unwrap();
    assert_eq!(result.mime_type, "second");
}
