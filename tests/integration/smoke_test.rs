use arcanum_core::config::ArcanumConfig;
use arcanum_engine::ArcanumEngineBuilder;
use arcanum_server::build_app;
use axum::http::{Request, StatusCode};
use axum::body::Body;
use tower::ServiceExt;

const TEST_SECRET: &str = "a-32-char-test-secret-for-testing!!";

#[tokio::test]
async fn test_full_stack_health_check() {
    let engine = ArcanumEngineBuilder::new(ArcanumConfig::default())
        .with_auth_secret(TEST_SECRET)
        .build().await.unwrap();

    let app = build_app(Some(engine));

    let resp = app.clone().oneshot(
        Request::builder().uri("/health").body(Body::empty()).unwrap()
    ).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let resp = app.oneshot(
        Request::builder().uri("/ready").body(Body::empty()).unwrap()
    ).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_unauthorized_search_rejected() {
    let engine = ArcanumEngineBuilder::new(ArcanumConfig::default())
        .with_auth_secret(TEST_SECRET)
        .build().await.unwrap();

    let app = build_app(Some(engine));
    let resp = app.oneshot(
        Request::builder()
            .method("POST")
            .uri("/api/v1/search")
            .header("content-type", "application/json")
            .body(Body::from(r#"{"query":"test","collection_id":"docs"}"#))
            .unwrap()
    ).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}
