use atlas::assemble_app_with_dist;
use atlas::demo::OllamaProbe;
use atlas::samples::load_manifest;
use axum::body::Body;
use axum::Router;
use http::{Request, StatusCode};
use serde_json::Value;
use std::path::Path;
use std::sync::Arc;
use tower::ServiceExt;

struct FakeProbe;

#[async_trait::async_trait]
impl OllamaProbe for FakeProbe {
    async fn tags(&self) -> Result<Vec<String>, String> {
        Ok(vec![])
    }
}

const INDEX: &str = "<html><body>ATLAS-SPA-INDEX</body></html>";

async fn send(router: &Router, req: Request<Body>) -> (StatusCode, String) {
    let resp = router.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    (status, String::from_utf8_lossy(&bytes).into_owned())
}

fn get(uri: &str) -> Request<Body> {
    Request::builder().uri(uri).body(Body::empty()).unwrap()
}

#[tokio::test]
async fn assembled_app_serves_api_and_demo() {
    let (state, dir) = crate::common::test_state().await;
    let manifest =
        Arc::new(load_manifest(&Path::new(env!("CARGO_MANIFEST_DIR")).join("samples")).unwrap());
    let dist = dir.path().join("dist");
    std::fs::create_dir_all(&dist).unwrap();
    std::fs::write(dist.join("index.html"), INDEX).unwrap();

    let app = assemble_app_with_dist(
        state.clone(),
        manifest.clone(),
        Arc::new(FakeProbe),
        Some(dist),
    );
    let key = state.admin_key.clone();

    assert_eq!(send(&app, get("/health")).await.0, StatusCode::OK);
    let (s, body) = send(&app, get("/demo/bootstrap")).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(
        serde_json::from_str::<Value>(&body).unwrap()["api_key"],
        key.as_str()
    );

    let search = Request::builder()
        .method("POST")
        .uri("/api/v1/search")
        .header("Authorization", format!("Bearer {key}"))
        .header("Content-Type", "application/json")
        .body(Body::from(r#"{"query":"leave","collection_id":"halcyon"}"#))
        .unwrap();
    let (s, body) = send(&app, search).await;
    assert_eq!(s, StatusCode::OK, "{body}");
    let v: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(v["chunks"].as_array().unwrap().len(), 0);

    let lib = Request::builder()
        .uri("/demo/library")
        .header("Authorization", format!("Bearer {key}"))
        .body(Body::empty())
        .unwrap();
    let (s, body) = send(&app, lib).await;
    assert_eq!(s, StatusCode::OK);
    let v: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(v["documents"].as_array().unwrap().len(), 0);

    // SPA fallback for client routes, but never for unknown API or demo paths.
    let (s, body) = send(&app, get("/library")).await;
    assert_eq!(s, StatusCode::OK);
    assert!(body.contains("ATLAS-SPA-INDEX"));
    for path in ["/demo/unknown", "/api/v1/unknown"] {
        let (s, body) = send(&app, get(path)).await;
        assert_eq!(s, StatusCode::NOT_FOUND, "{path}");
        assert!(!body.contains("ATLAS-SPA-INDEX"), "{path}");
    }

    // Without a dist dir there is no fallback at all.
    let bare = assemble_app_with_dist(state, manifest, Arc::new(FakeProbe), None);
    let (s, _) = send(&bare, get("/library")).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn cors_allows_only_the_vite_dev_origin() {
    let (state, _dir) = crate::common::test_state().await;
    let manifest =
        Arc::new(load_manifest(&Path::new(env!("CARGO_MANIFEST_DIR")).join("samples")).unwrap());
    let app = assemble_app_with_dist(state, manifest, Arc::new(FakeProbe), None);

    let from = |origin: &str| {
        Request::builder()
            .uri("/health")
            .header("Origin", origin)
            .body(Body::empty())
            .unwrap()
    };
    let allowed = app
        .clone()
        .oneshot(from("http://localhost:5173"))
        .await
        .unwrap();
    assert_eq!(
        allowed
            .headers()
            .get("access-control-allow-origin")
            .unwrap(),
        "http://localhost:5173"
    );
    let other = app.oneshot(from("http://evil.example")).await.unwrap();
    assert!(other.headers().get("access-control-allow-origin").is_none());
}
