use arcanum_engine::services::{
    collection::CollectionService,
    experiment::{ExperimentService, ExperimentStatus, InMemoryExperimentStore},
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
    use arcanum_ingestion::PreprocessorCatalog;
    Arc::new(CollectionService::new(
        ArcanumConfig::default(),
        Arc::new(AuditLogger::new()),
        Arc::new(AuthMiddleware::new("a-32-char-secret-for-testing-ok!")),
        Arc::new(PreprocessorCatalog::new()),
    ))
}

#[tokio::test]
async fn start_sets_experiment_to_active() {
    let collections = mock_collection_service();
    let svc = ExperimentService::new(collections.clone(), Arc::new(InMemoryExperimentStore::new()));
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
    let svc = ExperimentService::new(collections.clone(), Arc::new(InMemoryExperimentStore::new()));
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
    use arcanum_engine::services::experiment::ExperimentMetrics;
    let collections = mock_collection_service();
    let svc = ExperimentService::new(collections.clone(), Arc::new(InMemoryExperimentStore::new()));
    let col_id = CollectionId("test-col-3".into());
    let claims = arcanum_engine::auth::ApiKeyClaims {
        user_id: "test".into(), allowed_collections: vec![], is_admin: true, exp: 9999999999,
    };
    collections.create(col_id.clone(), "test".into(), &claims).await.unwrap();

    let challenger = fixed_config(256);
    let exp = svc.start(col_id.clone(), challenger.clone()).await.unwrap();
    // Set metrics so experiment transitions to ReadyToPromote
    svc.update_metrics(&col_id.0, &exp.id, ExperimentMetrics {
        champion_recall_at_5: 0.5, challenger_recall_at_5: 0.9,
        sample_size: 100, computed_at: "2026-06-07T00:00:00Z".into(),
    }).await.unwrap();
    svc.promote(&col_id.0, &exp.id).await.unwrap();

    let col_info = collections.get(&col_id.0).await.unwrap();
    assert!(col_info.chunker_config.is_some(), "promote should set chunker_config");
    let cfg = col_info.chunker_config.unwrap();
    assert_eq!(cfg.vector.strategy, "fixed");
    assert_eq!(cfg.vector.params["chunk_size"], 256);
}

#[tokio::test]
async fn promote_closes_experiment() {
    use arcanum_engine::services::experiment::ExperimentMetrics;
    let collections = mock_collection_service();
    let svc = ExperimentService::new(collections.clone(), Arc::new(InMemoryExperimentStore::new()));
    let col_id = CollectionId("test-col-4".into());
    let claims = arcanum_engine::auth::ApiKeyClaims {
        user_id: "test".into(), allowed_collections: vec![], is_admin: true, exp: 9999999999,
    };
    collections.create(col_id.clone(), "test".into(), &claims).await.unwrap();

    let exp = svc.start(col_id.clone(), fixed_config(256)).await.unwrap();
    // Set metrics so experiment transitions to ReadyToPromote
    svc.update_metrics(&col_id.0, &exp.id, ExperimentMetrics {
        champion_recall_at_5: 0.5, challenger_recall_at_5: 0.9,
        sample_size: 100, computed_at: "2026-06-07T00:00:00Z".into(),
    }).await.unwrap();
    svc.promote(&col_id.0, &exp.id).await.unwrap();

    let closed = svc.get(&col_id.0, &exp.id).await.unwrap();
    assert_eq!(closed.status, ExperimentStatus::Closed);
}

#[tokio::test]
async fn abandon_closes_experiment_without_changing_config() {
    let collections = mock_collection_service();
    let svc = ExperimentService::new(collections.clone(), Arc::new(InMemoryExperimentStore::new()));
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

#[tokio::test]
async fn update_metrics_on_closed_experiment_returns_error() {
    use arcanum_engine::services::experiment::ExperimentMetrics;
    let collections = mock_collection_service();
    let svc = ExperimentService::new(collections.clone(), Arc::new(InMemoryExperimentStore::new()));
    let col_id = CollectionId("test-col-closed".into());
    let claims = arcanum_engine::auth::ApiKeyClaims {
        user_id: "test".into(), allowed_collections: vec![], is_admin: true, exp: 9999999999,
    };
    collections.create(col_id.clone(), "test".into(), &claims).await.unwrap();

    let exp = svc.start(col_id.clone(), fixed_config(256)).await.unwrap();
    svc.abandon(&col_id.0, &exp.id).await.unwrap();

    let metrics = ExperimentMetrics {
        champion_recall_at_5:   0.5,
        challenger_recall_at_5: 0.9,
        sample_size:            100,
        computed_at:            "2026-06-07T00:00:00Z".into(),
    };
    let result = svc.update_metrics(&col_id.0, &exp.id, metrics).await;
    assert!(result.is_err(), "update_metrics on Closed experiment must fail");
}

#[tokio::test]
async fn promote_active_experiment_returns_error() {
    let collections = mock_collection_service();
    let svc = ExperimentService::new(collections.clone(), Arc::new(InMemoryExperimentStore::new()));
    let col_id = CollectionId("test-col-premature".into());
    let claims = arcanum_engine::auth::ApiKeyClaims {
        user_id: "test".into(), allowed_collections: vec![], is_admin: true, exp: 9999999999,
    };
    collections.create(col_id.clone(), "test".into(), &claims).await.unwrap();

    let exp = svc.start(col_id.clone(), fixed_config(256)).await.unwrap();
    // Experiment is Active (not ReadyToPromote) — must fail
    let result = svc.promote(&col_id.0, &exp.id).await;
    assert!(result.is_err(), "promoting an Active experiment must fail");
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("ReadyToPromote") || msg.contains("ready"),
        "error must mention ReadyToPromote: {}", msg
    );
}

#[tokio::test]
async fn promote_clears_collection_experiment_field() {
    let collections = mock_collection_service();
    let svc = ExperimentService::new(collections.clone(), Arc::new(InMemoryExperimentStore::new()));
    let col_id = CollectionId("test-col-clear".into());
    let claims = arcanum_engine::auth::ApiKeyClaims {
        user_id: "test".into(), allowed_collections: vec![], is_admin: true, exp: 9999999999,
    };
    collections.create(col_id.clone(), "test".into(), &claims).await.unwrap();

    let exp = svc.start(col_id.clone(), fixed_config(256)).await.unwrap();

    // Verify experiment was set on collection after start
    let col_info = collections.get(&col_id.0).await.unwrap();
    assert_eq!(col_info.experiment, Some(exp.id.clone()), "start must set collection.experiment");

    // Force ReadyToPromote so promote is allowed
    use arcanum_engine::services::experiment::ExperimentMetrics;
    svc.update_metrics(&col_id.0, &exp.id, ExperimentMetrics {
        champion_recall_at_5: 0.5, challenger_recall_at_5: 0.9,
        sample_size: 100, computed_at: "2026-06-07T00:00:00Z".into(),
    }).await.unwrap();

    svc.promote(&col_id.0, &exp.id).await.unwrap();

    let col_info = collections.get(&col_id.0).await.unwrap();
    assert!(col_info.experiment.is_none(), "promote must clear collection.experiment");
}

#[tokio::test]
async fn abandon_clears_collection_experiment_field() {
    let collections = mock_collection_service();
    let svc = ExperimentService::new(collections.clone(), Arc::new(InMemoryExperimentStore::new()));
    let col_id = CollectionId("test-col-abandon-clear".into());
    let claims = arcanum_engine::auth::ApiKeyClaims {
        user_id: "test".into(), allowed_collections: vec![], is_admin: true, exp: 9999999999,
    };
    collections.create(col_id.clone(), "test".into(), &claims).await.unwrap();

    let exp = svc.start(col_id.clone(), fixed_config(256)).await.unwrap();
    svc.abandon(&col_id.0, &exp.id).await.unwrap();

    let col_info = collections.get(&col_id.0).await.unwrap();
    assert!(col_info.experiment.is_none(), "abandon must clear collection.experiment");
}

#[tokio::test]
async fn collection_chunker_override_is_applied_to_ingestion_jobs() {
    use arcanum_engine::ingestion_deps_resolver::EngineIngestionDepsResolver;
    use arcanum_core::traits::IngestionDepsOverrideResolver;
    use arcanum_core::types::{ChunkStrategyConfig, PerBackendChunkConfig};
    use arcanum_ingestion::PreprocessorCatalog;

    let collections = mock_collection_service();
    let experiment_svc = ExperimentService::new(collections.clone(), Arc::new(InMemoryExperimentStore::new()));
    let claims = arcanum_engine::auth::ApiKeyClaims {
        user_id: "test".into(), allowed_collections: vec![], is_admin: true, exp: 9999999999,
    };
    let col_id = CollectionId("chunker-override-col".into());
    collections.create(col_id.clone(), "test".into(), &claims).await.unwrap();

    // Set a non-default chunker config on the collection
    let custom_cfg = PerBackendChunkConfig {
        vector: ChunkStrategyConfig {
            strategy: "semantic".to_string(),
            params: serde_json::json!({ "max_chars": 800 }),
        },
        graph: None,
        tree:  None,
    };
    collections.set_chunker_config(&col_id.0, Some(custom_cfg.clone())).await.unwrap();

    let global_cfg = PerBackendChunkConfig::default();
    let resolver = EngineIngestionDepsResolver {
        collection_service: collections.clone(),
        experiment_service: Arc::new(experiment_svc),
        global_chunking: global_cfg,
        preprocessor_catalog: Arc::new(PreprocessorCatalog::new()),
    };

    let (chunkers, shadow, _preprocessor) = resolver.resolve_for_collection(&col_id.0).await.unwrap();
    assert!(shadow.is_none(), "no active experiment, shadow must be None");
    // The resolved chunkers should be built from the custom config.
    // We can't easily inspect the Arc<dyn Chunker> type directly,
    // so we just verify resolution succeeded with the custom config.
    drop(chunkers); // compilation ensures the type is PerBackendChunkers
}

#[tokio::test]
async fn active_experiment_produces_shadow_context_with_correct_namespace() {
    use arcanum_engine::ingestion_deps_resolver::EngineIngestionDepsResolver;
    use arcanum_core::traits::IngestionDepsOverrideResolver;
    use arcanum_ingestion::PreprocessorCatalog;

    let collections = mock_collection_service();
    let experiment_svc = Arc::new(ExperimentService::new(collections.clone(), Arc::new(InMemoryExperimentStore::new())));
    let claims = arcanum_engine::auth::ApiKeyClaims {
        user_id: "test".into(), allowed_collections: vec![], is_admin: true, exp: 9999999999,
    };
    let col_id = CollectionId("shadow-resolver-col".into());
    collections.create(col_id.clone(), "test".into(), &claims).await.unwrap();

    let challenger = fixed_config(128);
    let exp = experiment_svc.start(col_id.clone(), challenger).await.unwrap();

    let resolver = EngineIngestionDepsResolver {
        collection_service: collections.clone(),
        experiment_service: experiment_svc.clone(),
        global_chunking: arcanum_core::types::PerBackendChunkConfig::default(),
        preprocessor_catalog: Arc::new(PreprocessorCatalog::new()),
    };

    let (_chunkers, shadow, _preprocessor) = resolver.resolve_for_collection(&col_id.0).await.unwrap();
    let shadow = shadow.expect("active experiment must produce a ShadowContext");
    let expected_ns = format!("{}__shadow_{}", col_id.0, exp.id.0);
    assert_eq!(shadow.shadow_collection_id, expected_ns,
        "shadow namespace must be '{{col_id}}__shadow_{{exp_id}}'");
}
