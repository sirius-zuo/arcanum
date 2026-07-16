use arcanum_engine::ingestion_deps_resolver::EngineIngestionDepsResolver;
use arcanum_engine::services::{collection::CollectionService, experiment::{ExperimentService, InMemoryExperimentStore}};
use arcanum_core::{
    config::ArcanumConfig,
    traits::{IngestionDepsOverrideResolver, Preprocessor},
    types::{CollectionId, PerBackendChunkConfig, RawDocument},
};
use arcanum_ingestion::PreprocessorCatalog;
use std::sync::Arc;

struct TaggingPreprocessor(&'static str);
#[async_trait::async_trait]
impl Preprocessor for TaggingPreprocessor {
    async fn process(&self, mut doc: RawDocument) -> arcanum_core::Result<RawDocument> {
        doc.mime_type = self.0.into();
        Ok(doc)
    }
}

fn admin_claims() -> arcanum_engine::auth::ApiKeyClaims {
    arcanum_engine::auth::ApiKeyClaims {
        user_id: "test-admin".into(),
        allowed_collections: vec![],
        is_admin: true,
        exp: 9999999999,
    }
}

fn make_resolver(catalog: PreprocessorCatalog) -> (Arc<CollectionService>, EngineIngestionDepsResolver) {
    use arcanum_engine::audit::AuditLogger;
    use arcanum_engine::auth::AuthMiddleware;
    let catalog = Arc::new(catalog);
    let collection = Arc::new(CollectionService::new(
        ArcanumConfig::default(),
        Arc::new(AuditLogger::new()),
        Arc::new(AuthMiddleware::new("a-32-char-secret-for-testing-ok!")),
        catalog.clone(),
    ));
    let experiment = Arc::new(ExperimentService::new(collection.clone(), Arc::new(InMemoryExperimentStore::new())));
    let resolver = EngineIngestionDepsResolver {
        collection_service: collection.clone(),
        experiment_service: experiment,
        global_chunking: PerBackendChunkConfig::default(),
        preprocessor_catalog: catalog,
    };
    (collection, resolver)
}

async fn tag_via(p: Arc<dyn Preprocessor>) -> String {
    let doc = RawDocument {
        id: arcanum_core::types::DocumentId::new(),
        content: b"x".to_vec(),
        mime_type: "text/plain".into(),
        source_uri: "test".into(),
        metadata: Default::default(),
    };
    p.process(doc).await.unwrap().mime_type
}

#[tokio::test]
async fn resolve_for_collection_uses_default_when_no_override() {
    let mut catalog = PreprocessorCatalog::new();
    catalog.register("default", Arc::new(TaggingPreprocessor("default-tag")));
    let (collection, resolver) = make_resolver(catalog);
    collection.create(CollectionId("col1".into()), "d".into(), &admin_claims()).await.unwrap();

    let (_, _, preprocessor) = resolver.resolve_for_collection("col1").await.unwrap();
    let tag = tag_via(preprocessor.expect("expected a resolved preprocessor")).await;
    assert_eq!(tag, "default-tag");
}

#[tokio::test]
async fn resolve_for_collection_uses_named_override() {
    let mut catalog = PreprocessorCatalog::new();
    catalog.register("default", Arc::new(TaggingPreprocessor("default-tag")));
    catalog.register("acme-edi", Arc::new(TaggingPreprocessor("acme-tag")));
    let (collection, resolver) = make_resolver(catalog);
    collection.create(CollectionId("col1".into()), "d".into(), &admin_claims()).await.unwrap();
    collection.set_preprocessor("col1", Some("acme-edi".into())).await.unwrap();

    let (_, _, preprocessor) = resolver.resolve_for_collection("col1").await.unwrap();
    let tag = tag_via(preprocessor.expect("expected a resolved preprocessor")).await;
    assert_eq!(tag, "acme-tag");
}

#[tokio::test]
async fn resolve_for_collection_returns_none_when_nothing_resolves() {
    let (collection, resolver) = make_resolver(PreprocessorCatalog::new());
    collection.create(CollectionId("col1".into()), "d".into(), &admin_claims()).await.unwrap();

    let (_, _, preprocessor) = resolver.resolve_for_collection("col1").await.unwrap();
    assert!(preprocessor.is_none());
}
