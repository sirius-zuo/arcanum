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
    std::env::set_var("ATLAS_PIPELINE", "standard");
    call(router, "POST", uri, Some(key)).await
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
