use arcanum_engine::services::collection::CollectionService;
use arcanum_core::{config::ArcanumConfig, types::CollectionId};
use arcanum_core::traits::Preprocessor;
use arcanum_ingestion::PreprocessorCatalog;
use std::sync::Arc;

struct StubPreprocessor;
#[async_trait::async_trait]
impl Preprocessor for StubPreprocessor {
    async fn process(&self, doc: arcanum_core::types::RawDocument) -> arcanum_core::Result<arcanum_core::types::RawDocument> {
        Ok(doc)
    }
}

fn mock_collection_service(catalog: PreprocessorCatalog) -> Arc<CollectionService> {
    use arcanum_engine::audit::AuditLogger;
    use arcanum_engine::auth::AuthMiddleware;
    Arc::new(CollectionService::new(
        ArcanumConfig::default(),
        Arc::new(AuditLogger::new()),
        Arc::new(AuthMiddleware::new("a-32-char-secret-for-testing-ok!")),
        Arc::new(catalog),
    ))
}

fn admin_claims() -> arcanum_engine::auth::ApiKeyClaims {
    arcanum_engine::auth::ApiKeyClaims {
        user_id: "test-admin".into(),
        allowed_collections: vec![],
        is_admin: true,
        exp: 9999999999,
    }
}

#[tokio::test]
async fn set_preprocessor_with_registered_name_succeeds() {
    let mut catalog = PreprocessorCatalog::new();
    catalog.register("acme-edi", Arc::new(StubPreprocessor));
    let svc = mock_collection_service(catalog);

    svc.create(CollectionId("col1".into()), "desc".into(), &admin_claims()).await.unwrap();
    svc.set_preprocessor("col1", Some("acme-edi".into())).await.unwrap();

    let info = svc.get("col1").await.unwrap();
    assert_eq!(info.preprocessor, Some("acme-edi".to_string()));
}

#[tokio::test]
async fn set_preprocessor_with_unregistered_name_returns_config_error() {
    let svc = mock_collection_service(PreprocessorCatalog::new());
    svc.create(CollectionId("col1".into()), "desc".into(), &admin_claims()).await.unwrap();

    let result = svc.set_preprocessor("col1", Some("nonexistent".into())).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("nonexistent"));

    // Collection must be unchanged.
    let info = svc.get("col1").await.unwrap();
    assert_eq!(info.preprocessor, None);
}

#[tokio::test]
async fn set_preprocessor_none_clears_override() {
    let mut catalog = PreprocessorCatalog::new();
    catalog.register("acme-edi", Arc::new(StubPreprocessor));
    let svc = mock_collection_service(catalog);

    svc.create(CollectionId("col1".into()), "desc".into(), &admin_claims()).await.unwrap();
    svc.set_preprocessor("col1", Some("acme-edi".into())).await.unwrap();
    svc.set_preprocessor("col1", None).await.unwrap();

    let info = svc.get("col1").await.unwrap();
    assert_eq!(info.preprocessor, None);
}
