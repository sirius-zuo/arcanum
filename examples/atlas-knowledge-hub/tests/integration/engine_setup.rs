use crate::common::{env_guard, models};
use atlas::{build_state, build_state_in, Settings};

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn build_state_wires_every_service() {
    let _env = env_guard();
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    let state = build_state(Settings::for_tests(data.clone()), models())
        .await
        .expect("build_state");

    let e = &state.engine;
    assert!(e.context.is_some());
    assert!(e.generate.is_some());
    assert!(e.verify.is_some());
    assert!(e.evidence.is_some());
    assert!(e.chunk_metadata_store.is_some());
    assert!(e.bm25_index.is_some());
    assert!(state.claims.is_admin);
    assert_eq!(state.metrics_token.len(), 32);

    let cols = e
        .vector_store
        .as_ref()
        .unwrap()
        .list_collections()
        .await
        .unwrap();
    assert!(cols.iter().any(|c| c == "halcyon"), "vector: {cols:?}");
    let g = e
        .graph_store
        .as_ref()
        .unwrap()
        .list_collections()
        .await
        .unwrap();
    assert!(g.iter().any(|c| c == "halcyon"), "graph: {g:?}");
    let t = e
        .tree_store
        .as_ref()
        .unwrap()
        .list_collections()
        .await
        .unwrap();
    assert!(t.iter().any(|c| c == "halcyon"), "tree: {t:?}");
    drop(state);

    // A second build with keep_data = false starts clean.
    let marker = data.join("marker.txt");
    std::fs::write(&marker, "x").unwrap();
    let again = build_state(Settings::for_tests(data.clone()), models()).await;
    assert!(again.is_ok());
    assert!(!marker.exists(), "data dir was not wiped");
}

#[test]
fn settings_from_env_defaults() {
    let _env = env_guard();
    for k in [
        "PORT",
        "MCP_PORT",
        "OLLAMA_URL",
        "ATLAS_CHAT_MODEL",
        "ATLAS_ENRICH_MODEL",
        "ANTHROPIC_API_KEY",
        "ARCANUM_AUTH_SECRET",
        "ATLAS_KEEP_DATA",
    ] {
        std::env::remove_var(k);
    }
    let s = Settings::from_env();
    assert_eq!(s.port, 8080);
    assert_eq!(s.mcp_port, 8081);
    assert_eq!(s.ollama_url, "http://localhost:11434");
    assert_eq!(s.chat_model, "qwen2.5");
    assert_eq!(s.enrich_model, "qwen2.5");
    assert!(s.anthropic_key.is_none());
    assert_eq!(s.auth_secret, "arcanum-dev-secret-minimum-32chars!!");
    assert!(!s.keep_data);
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn startup_refuses_and_wipes_nothing_outside_the_example_directory() {
    let _env = env_guard();
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("data")).unwrap();
    std::fs::write(dir.path().join("data/keep.txt"), "mine").unwrap();
    let err = build_state_in(dir.path(), Settings::for_tests("data".into()), models())
        .await
        .err()
        .expect("must refuse");
    assert!(format!("{err:#}").contains("config.toml"), "{err:#}");
    assert_eq!(
        std::fs::read_to_string(dir.path().join("data/keep.txt")).unwrap(),
        "mine"
    );

    // config.toml present but samples/ missing: still refused, still untouched.
    std::fs::write(dir.path().join("config.toml"), "").unwrap();
    let err = build_state_in(dir.path(), Settings::for_tests("data".into()), models())
        .await
        .err()
        .expect("must refuse");
    assert!(format!("{err:#}").contains("samples"), "{err:#}");
    assert!(dir.path().join("data/keep.txt").exists());
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn malformed_config_is_an_error_not_defaults() {
    let _env = env_guard();
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("samples")).unwrap();
    std::fs::write(dir.path().join("config.toml"), "this is = = not toml").unwrap();
    std::fs::create_dir_all(dir.path().join("data")).unwrap();
    std::fs::write(dir.path().join("data/keep.txt"), "mine").unwrap();
    let err = build_state_in(dir.path(), Settings::for_tests("data".into()), models())
        .await
        .err()
        .expect("must fail");
    assert!(format!("{err:#}").contains("config.toml"), "{err:#}");
    // The bad config is rejected before data/ is wiped.
    assert!(dir.path().join("data/keep.txt").exists());
}

#[test]
fn committed_config_toml_parses_and_carries_the_local_model_timeouts() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("config.toml");
    let cfg =
        arcanum_core::config::ArcanumConfig::from_file(&path).expect("config.toml must parse");
    assert_eq!(cfg.generate.first_token_timeout_secs, 120);
    assert_eq!(cfg.generate.total_timeout_secs, 300);
    assert_eq!(cfg.verify.judge_timeout_secs, 300);
    assert_eq!(cfg.ingestion.worker_pool_size, 2);
}
