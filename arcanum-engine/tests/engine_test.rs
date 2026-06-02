use arcanum_engine::ArcanumEngineBuilder;
use arcanum_core::config::*;
use arcanum_engine::services::ingestion::{IngestionService, IngestRequest};
use arcanum_ingestion::DocumentHashTracker;
use arcanum_core::types::CollectionId;
use std::sync::Arc;

const TEST_SECRET: &str = "a-32-char-test-secret-for-testing!!";

#[tokio::test]
async fn test_engine_build_fails_with_sqlite_in_production() {
    let mut cfg = ArcanumConfig::default();
    cfg.global.runtime_mode = RuntimeMode::Production;
    cfg.storage.metadata_backend = MetadataBackend::Sqlite;
    let result = ArcanumEngineBuilder::new(cfg)
        .with_auth_secret(TEST_SECRET)
        .build().await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("SQLite"));
}

#[tokio::test]
async fn test_engine_build_succeeds_in_development() {
    let cfg = ArcanumConfig::default();
    let engine = ArcanumEngineBuilder::new(cfg)
        .with_auth_secret(TEST_SECRET)
        .build().await;
    assert!(engine.is_ok());
}

#[tokio::test]
async fn test_engine_build_fails_without_secret() {
    // Ensure env var is unset so we get the "no secret" error path.
    std::env::remove_var("ARCANUM_AUTH_SECRET");
    let cfg = ArcanumConfig::default();
    let result = ArcanumEngineBuilder::new(cfg).build().await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("ARCANUM_AUTH_SECRET"));
}

#[tokio::test]
async fn test_engine_build_fails_with_short_secret() {
    let cfg = ArcanumConfig::default();
    let result = ArcanumEngineBuilder::new(cfg)
        .with_auth_secret("too-short")
        .build().await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("32 characters"));
}

#[tokio::test]
async fn test_ingest_skips_already_seen_uri() {
    let tracker = Arc::new(DocumentHashTracker::new());
    tracker.record("file:///doc.pdf", b"some content").await;

    let svc = IngestionService::new_with_tracker(tracker);

    let req = IngestRequest {
        source_uri: "file:///doc.pdf".into(),
        collection_id: CollectionId("col".into()),
        pipeline_template: None,
        force: false,
        content: None,
        mime_hint: None,
    };
    let result = svc.ingest(req, "user1").await;
    assert!(result.is_ok());
}
