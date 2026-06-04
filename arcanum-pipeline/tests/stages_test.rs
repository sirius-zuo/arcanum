use arcanum_pipeline::dag::{CTX_FORCE, CTX_REPLACE, CTX_SKIP};
use arcanum_pipeline::stages::make_load_stage;
use arcanum_core::traits::Source;
use arcanum_core::types::CollectionId;
use arcanum_pipeline::IngestionState;
use arcanum_ingestion::{LoaderRegistry, RawLoader};
use std::sync::Arc;

#[test]
fn ctx_constants_have_expected_values() {
    assert_eq!(CTX_FORCE,   "__force");
    assert_eq!(CTX_SKIP,    "__skip");
    assert_eq!(CTX_REPLACE, "__replace");
}

#[tokio::test]
async fn test_make_load_stage_id_is_load() {
    let loaders = Arc::new(LoaderRegistry::new().register(Arc::new(RawLoader::new())));
    let state = Arc::new(tokio::sync::Mutex::new(IngestionState::new(
        Source::Raw { content: b"test".to_vec(), mime_hint: None, uri: "raw://x".into() },
        CollectionId("col".into()),
    )));
    let stage = make_load_stage(state, loaders);
    assert_eq!(stage.id, "load");
    assert!(stage.deps.is_empty());
}
