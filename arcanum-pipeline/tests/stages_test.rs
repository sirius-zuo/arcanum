use arcanum_pipeline::stages::make_load_stage;
use arcanum_core::traits::Source;
use arcanum_core::types::CollectionId;
use arcanum_pipeline::IngestionState;
use arcanum_ingestion::{LoaderRegistry, RawLoader, DocumentHashTracker};
use std::sync::Arc;

#[tokio::test]
async fn test_make_load_stage_id_is_load() {
    let loaders = Arc::new(LoaderRegistry::new().register(Arc::new(RawLoader::new())));
    let tracker = Arc::new(DocumentHashTracker::new());
    let state = Arc::new(tokio::sync::Mutex::new(IngestionState::new(
        Source::Raw { content: b"test".to_vec(), mime_hint: None, uri: "raw://x".into() },
        CollectionId("col".into()),
    )));
    let stage = make_load_stage(state, loaders, tracker);
    assert_eq!(stage.id, "load");
    assert!(stage.deps.is_empty());
}
