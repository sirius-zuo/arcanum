use crate::common::{env_guard, test_state};
use arcanum_core::types::{OperationId, OperationStatus};
use atlas::demo::{demo_router, OllamaProbe};
use atlas::samples::load_manifest;
use atlas::AtlasState;
use axum::body::Body;
use axum::Router;
use http::{Request, StatusCode};
use serde_json::Value;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tower::ServiceExt;

struct NoProbe;

#[async_trait::async_trait]
impl OllamaProbe for NoProbe {
    async fn tags(&self) -> Result<Vec<String>, String> {
        Ok(vec![])
    }
}

async fn setup() -> (Arc<AtlasState>, Router, String, tempfile::TempDir) {
    let (state, dir) = test_state().await;
    let manifest = load_manifest(&Path::new(env!("CARGO_MANIFEST_DIR")).join("samples")).unwrap();
    let router = demo_router(state.clone(), Arc::new(manifest), Arc::new(NoProbe));
    let key = state.admin_key.clone();
    (state, router, key, dir)
}

async fn call(router: &Router, method: &str, uri: &str, key: Option<&str>) -> (StatusCode, Value) {
    let mut req = Request::builder().method(method).uri(uri);
    if let Some(k) = key {
        req = req.header("Authorization", format!("Bearer {k}"));
    }
    let resp = router
        .clone()
        .oneshot(req.body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

/// POSTs with the `standard` pipeline template (no enricher needed).
#[allow(clippy::await_holding_lock)]
async fn post_standard(router: &Router, uri: &str, key: &str) -> (StatusCode, Value) {
    let _env = env_guard();
    let previous = std::env::var_os("ATLAS_PIPELINE");
    std::env::set_var("ATLAS_PIPELINE", "standard");
    let out = call(router, "POST", uri, Some(key)).await;
    match previous {
        Some(v) => std::env::set_var("ATLAS_PIPELINE", v),
        None => std::env::remove_var("ATLAS_PIPELINE"),
    }
    out
}

fn op_ids(body: &Value) -> Vec<String> {
    body["operations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o["operation_id"].as_str().unwrap().to_string())
        .collect()
}

async fn wait_all(state: &AtlasState, ids: &[String]) {
    let ops = state.engine.ingestion.operations();
    let deadline = Instant::now() + Duration::from_secs(60);
    for id in ids {
        let id = OperationId(id.parse().unwrap());
        loop {
            let op = ops.get(&id).await.unwrap().unwrap();
            if matches!(
                op.status,
                OperationStatus::Succeeded | OperationStatus::Failed
            ) {
                assert_eq!(op.status, OperationStatus::Succeeded, "{op:?}");
                break;
            }
            assert!(Instant::now() < deadline, "operation timed out: {op:?}");
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }
}

#[tokio::test]
async fn load_submits_ten_operations() {
    let (state, router, key, _dir) = setup().await;
    let (status, body) = post_standard(&router, "/demo/samples/load", &key).await;
    assert_eq!(status, StatusCode::ACCEPTED);
    let ids = op_ids(&body);
    assert_eq!(ids.len(), 10);
    let mut distinct = ids.clone();
    distinct.sort();
    distinct.dedup();
    assert_eq!(distinct.len(), 10);
    wait_all(&state, &ids).await;
}

#[tokio::test]
async fn load_is_idempotent() {
    let (_state, router, key, _dir) = setup().await;
    let (s1, b1) = post_standard(&router, "/demo/samples/load", &key).await;
    assert_eq!(s1, StatusCode::ACCEPTED);
    let (s2, b2) = post_standard(&router, "/demo/samples/load", &key).await;
    assert_eq!(s2, StatusCode::OK);
    assert_eq!(op_ids(&b1), op_ids(&b2));
}

#[tokio::test]
async fn apply_update_creates_version_two() {
    let (state, router, key, _dir) = setup().await;
    let (_, body) = post_standard(&router, "/demo/samples/load", &key).await;
    wait_all(&state, &op_ids(&body)).await;
    let (status, body) = post_standard(&router, "/demo/samples/apply-update", &key).await;
    assert_eq!(status, StatusCode::ACCEPTED);
    let ids = op_ids(&body);
    assert_eq!(ids.len(), 1);
    assert_eq!(body["operations"][0]["source_uri"], "security-policy.md");
    wait_all(&state, &ids).await;
    let (_, lib) = call(&router, "GET", "/demo/library", Some(&key)).await;
    let doc = lib["documents"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["source_uri"] == "security-policy.md")
        .expect("security-policy.md in library");
    assert_eq!(doc["versions"].as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn ingest_requires_key() {
    let (_state, router, _key, _dir) = setup().await;
    for uri in ["/demo/samples/load", "/demo/samples/apply-update"] {
        let (status, _) = call(&router, "POST", uri, None).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }
}

/// Fresh state, load all ten samples at once, every operation must succeed. Repeated with
/// fresh states to show the first-use table race is gone.
#[tokio::test]
async fn concurrent_first_ingests_all_succeed() {
    for _ in 0..3 {
        let (state, router, key, _dir) = setup().await;
        let (status, body) = post_standard(&router, "/demo/samples/load", &key).await;
        assert_eq!(status, StatusCode::ACCEPTED);
        let ids = op_ids(&body);
        assert_eq!(ids.len(), 10);
        wait_all(&state, &ids).await;
    }
}

#[tokio::test]
async fn warmup_leaves_no_data() {
    use arcanum_core::types::{CollectionId, Query};
    let (state, _dir) = test_state().await;
    let result = state
        .engine
        .retrieval
        .search(
            Query::new("atlas warm-up sentinel")
                .with_collection(CollectionId("halcyon".into()))
                .with_top_k(10),
            &state.claims,
        )
        .await
        .unwrap();
    assert!(result.chunks.is_empty(), "{:?}", result.chunks.len());
    assert!(state.registry.get_all().await.is_empty());
}

/// Like `FakeEmbedder` but, like `OllamaProvider`, reports dimension 0.
struct ZeroDimEmbedder;

#[async_trait::async_trait]
impl arcanum_core::traits::Embedder for ZeroDimEmbedder {
    async fn embed(
        &self,
        texts: Vec<String>,
    ) -> arcanum_core::Result<Vec<arcanum_core::types::Vector>> {
        crate::common::FakeEmbedder.embed(texts).await
    }
    fn dimension(&self) -> usize {
        0
    }
}

#[allow(clippy::await_holding_lock)]
async fn state_with(embed_dimension: usize, dir: &tempfile::TempDir) -> anyhow::Result<AtlasState> {
    let mut m = crate::common::models();
    m.embedder = Arc::new(ZeroDimEmbedder);
    m.embed_dimension = embed_dimension;
    let _env = env_guard();
    atlas::build_state(atlas::Settings::for_tests(dir.path().join("data")), m).await
}

#[tokio::test]
async fn warmup_uses_configured_dimension_not_embedder_dimension() {
    let dir = tempfile::tempdir().unwrap();
    let state = state_with(8, &dir).await.unwrap();
    let bytes = std::fs::read("samples/employee-handbook.md").unwrap();
    let report =
        crate::common::ingest_and_wait(&state, "employee-handbook.md", &bytes, "standard").await;
    assert_eq!(report.status, OperationStatus::Succeeded, "{report:?}");
}

#[tokio::test]
async fn warmup_rejects_zero_dimension() {
    let dir = tempfile::tempdir().unwrap();
    let err = state_with(0, &dir).await.err().expect("must fail");
    let msg = format!("{err:#}");
    assert!(msg.contains("dimension"), "{msg}");
}
