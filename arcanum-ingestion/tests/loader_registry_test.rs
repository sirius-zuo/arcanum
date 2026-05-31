use arcanum_core::traits::{DocumentLoader, Source};
use arcanum_core::types::*;
use arcanum_core::{Result, ArcanumError};
use arcanum_ingestion::loaders::LoaderRegistry;
use async_trait::async_trait;
use std::sync::Arc;

struct AlwaysLoader;
#[async_trait]
impl DocumentLoader for AlwaysLoader {
    async fn load(&self, source: &Source) -> Result<RawDocument> {
        Ok(RawDocument {
            id: DocumentId::new(), content: b"data".to_vec(),
            mime_type: "text/plain".into(), source_uri: source.uri().to_string(),
            metadata: Default::default(),
        })
    }
    fn supports(&self, _: &Source) -> bool { true }
}

struct NeverLoader;
#[async_trait]
impl DocumentLoader for NeverLoader {
    async fn load(&self, _: &Source) -> Result<RawDocument> {
        Err(ArcanumError::Ingestion("never".into()))
    }
    fn supports(&self, _: &Source) -> bool { false }
}

#[tokio::test]
async fn test_registry_routes_to_first_supporting_loader() {
    let reg = LoaderRegistry::new()
        .register(Arc::new(NeverLoader))
        .register(Arc::new(AlwaysLoader));
    let doc = reg.load(&Source::Url("https://x.com".into())).await.unwrap();
    assert_eq!(doc.content, b"data");
}

#[tokio::test]
async fn test_registry_errors_when_no_loader_matches() {
    let reg = LoaderRegistry::new().register(Arc::new(NeverLoader));
    assert!(reg.load(&Source::Url("https://x.com".into())).await.is_err());
}

#[tokio::test]
async fn test_empty_registry_errors() {
    let reg = LoaderRegistry::new();
    assert!(reg.load(&Source::File("/tmp/x".into())).await.is_err());
}
