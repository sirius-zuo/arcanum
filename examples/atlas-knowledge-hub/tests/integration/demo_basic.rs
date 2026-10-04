use atlas::demo::{demo_router, OllamaProbe};
use atlas::samples::load_manifest;
use axum::body::Body;
use axum::Router;
use http::{Request, StatusCode};
use serde_json::Value;
use std::path::Path;
use std::sync::Arc;
use tower::ServiceExt;

struct FakeProbe(Result<Vec<String>, String>);

#[async_trait::async_trait]
impl OllamaProbe for FakeProbe {
    async fn tags(&self) -> Result<Vec<String>, String> {
        self.0.clone()
    }
}

async fn app(probe: Result<Vec<String>, String>) -> (Router, tempfile::TempDir) {
    let (state, dir) = crate::common::test_state().await;
    let manifest = load_manifest(&Path::new(env!("CARGO_MANIFEST_DIR")).join("samples")).unwrap();
    let router = demo_router(state, Arc::new(manifest), Arc::new(FakeProbe(probe)));
    (router, dir)
}

async fn get(router: &Router, uri: &str, key: Option<&str>) -> (StatusCode, Value) {
    let mut req = Request::builder().uri(uri);
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

async fn key(router: &Router) -> String {
    let (_, b) = get(router, "/demo/bootstrap", None).await;
    b["api_key"].as_str().unwrap().to_string()
}

fn tags(v: &[&str]) -> Result<Vec<String>, String> {
    Ok(v.iter().map(|s| s.to_string()).collect())
}

fn check<'a>(h: &'a Value, id: &str) -> &'a Value {
    h["checks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == id)
        .unwrap_or_else(|| panic!("no check {id}"))
}

async fn bootstrap_status(router: &Router, host: Option<&str>) -> StatusCode {
    let mut req = Request::builder().uri("/demo/bootstrap");
    if let Some(h) = host {
        req = req.header("Host", h);
    }
    router
        .clone()
        .oneshot(req.body(Body::empty()).unwrap())
        .await
        .unwrap()
        .status()
}

#[tokio::test]
async fn bootstrap_refuses_non_local_host_headers_on_a_loopback_bind() {
    let (r, _d) = app(tags(&[])).await;
    for ok in ["localhost:8080", "127.0.0.1:8080", "[::1]:8080"] {
        assert_eq!(bootstrap_status(&r, Some(ok)).await, StatusCode::OK, "{ok}");
    }
    for bad in ["evil.example", "evil.example:8080", "192.168.1.9:8080"] {
        assert_eq!(
            bootstrap_status(&r, Some(bad)).await,
            StatusCode::FORBIDDEN,
            "{bad}"
        );
    }
}

#[tokio::test]
async fn bootstrap_host_check_is_off_when_the_bind_was_widened() {
    let (state, _dir) = crate::common::test_state().await;
    let mut settings = state.settings.clone();
    settings.host = "0.0.0.0".into();
    let widened = Arc::new(atlas::AtlasState {
        settings,
        ..(*state).clone()
    });
    let manifest = load_manifest(&Path::new(env!("CARGO_MANIFEST_DIR")).join("samples")).unwrap();
    let r = demo_router(widened, Arc::new(manifest), Arc::new(FakeProbe(tags(&[]))));
    assert_eq!(
        bootstrap_status(&r, Some("10.0.0.5:8080")).await,
        StatusCode::OK
    );
}

#[tokio::test]
async fn bootstrap_needs_no_auth_and_matches_contract() {
    let (r, _d) = app(tags(&[])).await;
    let (st, b) = get(&r, "/demo/bootstrap", None).await;
    assert_eq!(st, StatusCode::OK);
    assert!(b["api_key"].as_str().unwrap().len() > 10);
    assert_eq!(b["collection"], "halcyon");
    assert_eq!(b["orchestration_mode"], "ParallelFusion");
    assert_eq!(b["generators"][0]["name"], "local");
    assert_eq!(b["generators"][0]["is_default"], true);
    assert_eq!(b["judge"], "local");
    assert_eq!(b["anthropic_enabled"], false);
    assert_eq!(b["mcp_port"], 8081);
    assert_eq!(b["features"]["gc"], false);
    assert_eq!(b["features"]["experiments"], true);
    assert_eq!(b["features"]["generate"], true);
}

#[tokio::test]
async fn health_requires_key() {
    let (r, _d) = app(tags(&[])).await;
    let (st, _) = get(&r, "/demo/health", None).await;
    assert_eq!(st, StatusCode::UNAUTHORIZED);
    let (st, _) = get(&r, "/demo/health", Some("bogus")).await;
    assert_eq!(st, StatusCode::UNAUTHORIZED);
    let k = key(&r).await;
    let (st, _) = get(&r, "/demo/health", Some(&k)).await;
    assert_eq!(st, StatusCode::OK);
}

#[tokio::test]
async fn health_reports_missing_model_with_fix() {
    let (r, _d) = app(tags(&["nomic-embed-text:latest"])).await;
    let k = key(&r).await;
    let (_, h) = get(&r, "/demo/health", Some(&k)).await;
    let chat = check(&h, "chat_model");
    assert_eq!(chat["ok"], false);
    assert!(chat["fix"]
        .as_str()
        .unwrap()
        .contains("ollama pull qwen2.5"));
    assert_eq!(check(&h, "embed_model")["ok"], true);
    assert_eq!(h["ready"], false);
}

#[tokio::test]
async fn health_ready_when_models_present() {
    let (r, _d) = app(tags(&["nomic-embed-text:latest", "qwen2.5:latest"])).await;
    let k = key(&r).await;
    let (_, h) = get(&r, "/demo/health", Some(&k)).await;
    assert_eq!(h["ready"], true, "{h}");
    assert_eq!(h["checks"].as_array().unwrap().len(), 11);
    assert!(check(&h, "ollama")["fix"].is_null());
}

#[tokio::test]
async fn health_when_ollama_down() {
    let (r, _d) = app(Err("connection refused".into())).await;
    let k = key(&r).await;
    let (_, h) = get(&r, "/demo/health", Some(&k)).await;
    let o = check(&h, "ollama");
    assert_eq!(o["ok"], false);
    assert!(o["fix"].as_str().unwrap().contains("ollama serve"));
    assert_eq!(check(&h, "embed_model")["ok"], false);
    assert_eq!(check(&h, "chat_model")["ok"], false);
    assert_eq!(h["ready"], false);
}

#[tokio::test]
async fn samples_returns_manifest() {
    let (r, _d) = app(tags(&[])).await;
    let k = key(&r).await;
    let (st, _) = get(&r, "/demo/samples", None).await;
    assert_eq!(st, StatusCode::UNAUTHORIZED);
    let (st, s) = get(&r, "/demo/samples", Some(&k)).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(s["files"].as_array().unwrap().len(), 11);
    assert_eq!(s["golden"].as_array().unwrap().len(), 12);
    assert_eq!(s["flawed_answers"].as_array().unwrap().len(), 3);
    assert_eq!(s["tour"].as_array().unwrap().len(), 9);
}
