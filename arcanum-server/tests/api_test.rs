use axum::http::{Request, StatusCode};
use axum::body::Body;
use tower::ServiceExt;
use arcanum_server::build_app;

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
    let status = post_json("/api/v1/search",
        serde_json::json!({ "query": "test", "collection_id": "docs" })
    ).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_ws_route_exists() {
    // Without upgrade headers → 400, not 404.
    let status = get("/ws/events").await;
    assert_ne!(status, StatusCode::NOT_FOUND);
}
