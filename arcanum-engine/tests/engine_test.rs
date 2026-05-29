use arcanum_engine::ArcanumEngineBuilder;
use arcanum_core::config::*;

#[tokio::test]
async fn test_engine_build_fails_with_sqlite_in_production() {
    let mut cfg = ArcanumConfig::default();
    cfg.global.runtime_mode = RuntimeMode::Production;
    cfg.storage.metadata_backend = MetadataBackend::Sqlite;
    let result = ArcanumEngineBuilder::new(cfg).build().await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("SQLite"));
}

#[tokio::test]
async fn test_engine_build_succeeds_in_development() {
    let cfg = ArcanumConfig::default();
    let engine = ArcanumEngineBuilder::new(cfg).build().await;
    assert!(engine.is_ok());
}
