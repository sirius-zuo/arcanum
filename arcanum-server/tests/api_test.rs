use arcanum_server::build_app;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

async fn get(uri: &str) -> StatusCode {
    let app = build_app(None);
    let req = Request::builder().uri(uri).body(Body::empty()).unwrap();
    app.oneshot(req).await.unwrap().status()
}

async fn post_json(uri: &str, body: serde_json::Value) -> StatusCode {
    let app = build_app(None);
    let req = Request::builder()
        .method("POST")
        .uri(uri)
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();
    app.oneshot(req).await.unwrap().status()
}

#[tokio::test]
async fn test_health_endpoint_returns_200() {
    assert_eq!(get("/health").await, StatusCode::OK);
}

#[tokio::test]
async fn test_ready_endpoint_returns_200() {
    assert_eq!(get("/ready").await, StatusCode::OK);
}

#[tokio::test]
async fn test_search_requires_auth() {
    let status = post_json(
        "/api/v1/search",
        serde_json::json!({ "query": "test", "collection_id": "docs" }),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_context_requires_auth() {
    let status = post_json(
        "/api/v1/context",
        serde_json::json!({ "collection_id": "docs", "query": "test" }),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

/// POSTs to /api/v1/context as an authenticated admin on an engine built
/// without a database, so it has no chunk registry.
async fn post_context_authed(body: serde_json::Value) -> (StatusCode, serde_json::Value) {
    use arcanum_core::traits::NoOpDocumentVersionStore;
    use arcanum_engine::ArcanumEngine;
    use std::sync::Arc;
    let engine = ArcanumEngine::builder()
        .auth_secret("a-32-char-secret-for-testing-ok!")
        .version_store(Arc::new(NoOpDocumentVersionStore))
        .build()
        .await
        .unwrap();
    let token = engine.auth.generate_admin_key("tester");
    let app = build_app(Some(engine));
    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/context")
        .header("Authorization", format!("Bearer {token}"))
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    (status, serde_json::from_slice(&bytes).unwrap())
}

#[tokio::test]
async fn test_context_unknown_role_is_400() {
    let (status, _) = post_context_authed(serde_json::json!({
        "collection_id": "docs",
        "messages": [{ "role": "system", "content": "hi" }],
    }))
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_context_without_chunk_registry_is_503() {
    let (status, body) =
        post_context_authed(serde_json::json!({ "collection_id": "docs", "query": "test" })).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["error"], "context requires a chunk registry");
}

#[tokio::test]
async fn test_ws_route_exists() {
    // Without upgrade headers → 400, not 404.
    let status = get("/ws/events").await;
    assert_ne!(status, StatusCode::NOT_FOUND);
}
