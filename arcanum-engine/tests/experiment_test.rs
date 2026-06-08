use arcanum_engine::services::{
    collection::CollectionService,
    experiment::{ExperimentService, ExperimentStatus},
};
use arcanum_core::{
    config::ArcanumConfig,
    types::{CollectionId, ChunkStrategyConfig, PerBackendChunkConfig},
};
use std::sync::Arc;

fn fixed_config(size: u64) -> PerBackendChunkConfig {
    PerBackendChunkConfig {
        vector: ChunkStrategyConfig {
            strategy: "fixed".to_string(),
            params: serde_json::json!({ "chunk_size": size, "overlap": 8 }),
        },
        graph: None,
        tree:  None,
    }
}

fn mock_collection_service() -> Arc<CollectionService> {
    use arcanum_engine::audit::AuditLogger;
    use arcanum_engine::auth::AuthMiddleware;
    Arc::new(CollectionService::new(
        ArcanumConfig::default(),
        Arc::new(AuditLogger::new()),
        Arc::new(AuthMiddleware::new("a-32-char-secret-for-testing-ok!")),
    ))
}

#[tokio::test]
async fn start_sets_experiment_to_active() {
    let collections = mock_collection_service();
    let svc = ExperimentService::new(collections.clone());
    let col_id = CollectionId("test-col".into());
    // Pre-create the collection
    let claims = arcanum_engine::auth::ApiKeyClaims {
        user_id: "test".into(), allowed_collections: vec![], is_admin: true, exp: 9999999999,
    };
    collections.create(col_id.clone(), "test".into(), &claims).await.unwrap();

    let exp = svc.start(col_id.clone(), fixed_config(256)).await.unwrap();
    assert_eq!(exp.status, ExperimentStatus::Active);
}

#[tokio::test]
async fn starting_second_experiment_while_active_returns_conflict() {
    let collections = mock_collection_service();
    let svc = ExperimentService::new(collections.clone());
    let col_id = CollectionId("test-col-2".into());
    let claims = arcanum_engine::auth::ApiKeyClaims {
        user_id: "test".into(), allowed_collections: vec![], is_admin: true, exp: 9999999999,
    };
    collections.create(col_id.clone(), "test".into(), &claims).await.unwrap();

    svc.start(col_id.clone(), fixed_config(256)).await.unwrap();
    let result = svc.start(col_id.clone(), fixed_config(512)).await;
    assert!(result.is_err(), "second start while active should fail");
    let msg = result.unwrap_err().to_string();
    assert!(msg.contains("active") || msg.contains("conflict") || msg.contains("already"),
        "error message should mention conflict: {}", msg);
}

#[tokio::test]
async fn promote_updates_collection_chunker_config() {
    let collections = mock_collection_service();
    let svc = ExperimentService::new(collections.clone());
    let col_id = CollectionId("test-col-3".into());
    let claims = arcanum_engine::auth::ApiKeyClaims {
        user_id: "test".into(), allowed_collections: vec![], is_admin: true, exp: 9999999999,
    };
    collections.create(col_id.clone(), "test".into(), &claims).await.unwrap();

    let challenger = fixed_config(256);
    let exp = svc.start(col_id.clone(), challenger.clone()).await.unwrap();
    svc.promote(&col_id.0, &exp.id).await.unwrap();

    let col_info = collections.get(&col_id.0).await.unwrap();
    assert!(col_info.chunker_config.is_some(), "promote should set chunker_config");
    let cfg = col_info.chunker_config.unwrap();
    assert_eq!(cfg.vector.strategy, "fixed");
    assert_eq!(cfg.vector.params["chunk_size"], 256);
}

#[tokio::test]
async fn promote_closes_experiment() {
    let collections = mock_collection_service();
    let svc = ExperimentService::new(collections.clone());
    let col_id = CollectionId("test-col-4".into());
    let claims = arcanum_engine::auth::ApiKeyClaims {
        user_id: "test".into(), allowed_collections: vec![], is_admin: true, exp: 9999999999,
    };
    collections.create(col_id.clone(), "test".into(), &claims).await.unwrap();

    let exp = svc.start(col_id.clone(), fixed_config(256)).await.unwrap();
    svc.promote(&col_id.0, &exp.id).await.unwrap();

    let closed = svc.get(&col_id.0, &exp.id).await.unwrap();
    assert_eq!(closed.status, ExperimentStatus::Closed);
}

#[tokio::test]
async fn abandon_closes_experiment_without_changing_config() {
    let collections = mock_collection_service();
    let svc = ExperimentService::new(collections.clone());
    let col_id = CollectionId("test-col-5".into());
    let claims = arcanum_engine::auth::ApiKeyClaims {
        user_id: "test".into(), allowed_collections: vec![], is_admin: true, exp: 9999999999,
    };
    collections.create(col_id.clone(), "test".into(), &claims).await.unwrap();

    let exp = svc.start(col_id.clone(), fixed_config(256)).await.unwrap();
    svc.abandon(&col_id.0, &exp.id).await.unwrap();

    let col_info = collections.get(&col_id.0).await.unwrap();
    assert!(col_info.chunker_config.is_none(), "abandon must not change chunker_config");

    let closed = svc.get(&col_id.0, &exp.id).await.unwrap();
    assert_eq!(closed.status, ExperimentStatus::Closed);
}
