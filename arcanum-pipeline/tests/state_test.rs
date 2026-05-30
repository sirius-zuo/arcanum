use arcanum_pipeline::{IngestionState, StageFailure};
use arcanum_core::traits::Source;
use arcanum_core::types::CollectionId;
use std::path::PathBuf;

#[test]
fn test_ingestion_state_starts_empty() {
    let state = IngestionState::new(
        Source::File(PathBuf::from("/tmp/x")),
        CollectionId("col1".into()),
    );
    assert!(state.doc.is_none());
    assert!(state.chunks.is_empty());
    assert!(state.vectors.is_empty());
}

#[test]
fn test_stage_failure_is_core_or_noncore() {
    use arcanum_core::ArcanumError;
    let f = StageFailure::Core { stage: "load".into(), error: ArcanumError::Ingestion("x".into()) };
    assert!(matches!(f, StageFailure::Core { .. }));
}
