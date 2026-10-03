use arcanum_core::traits::{Embedder, Generator, ScriptStep, ScriptedGenerator, StopReason};
use arcanum_core::types::Vector;
use atlas::{build_state, ModelDeps, Settings};
use std::sync::Arc;

struct FakeEmbedder;

#[async_trait::async_trait]
impl Embedder for FakeEmbedder {
    async fn embed(&self, texts: Vec<String>) -> arcanum_core::Result<Vec<Vector>> {
        Ok(texts
            .iter()
            .map(|t| {
                let mut v = vec![0.0f32; 8];
                for (i, b) in t.bytes().enumerate() {
                    v[i % 8] += b as f32;
                }
                Vector(v)
            })
            .collect())
    }
    fn dimension(&self) -> usize {
        8
    }
}

fn models() -> ModelDeps {
    let gen: Arc<dyn Generator> = Arc::new(ScriptedGenerator::new(
        "m",
        vec![
            ScriptStep::Delta("ok".into()),
            ScriptStep::Done(StopReason::EndTurn),
        ],
    ));
    ModelDeps {
        embedder: Arc::new(FakeEmbedder),
        enricher: None,
        generators: vec![("local".into(), gen, 512)],
        default_generator: "local".into(),
        judge: Some("local".into()),
    }
}

#[tokio::test]
async fn build_state_wires_every_service() {
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
