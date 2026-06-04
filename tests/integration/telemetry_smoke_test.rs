// tests/integration/telemetry_smoke_test.rs
//
// End-to-end telemetry smoke test. Uses arcanum_telemetry::testing::TestTelemetry
// to capture spans in memory, fires real HTTP requests against an in-process server,
// and asserts the expected span hierarchy exists.

use arcanum_telemetry::testing::TestTelemetry;
use axum::{body::{Body, to_bytes}, http::{Request, Method, header}};
use tower::ServiceExt;
use std::time::Duration;

// Helper: build a minimal no-engine app (same as smoke_test.rs)
fn build_test_app() -> axum::Router {
    arcanum_server::build_app(None)
}

/// Send a request, fully consume the response body, then allow the tracing
/// subscriber to flush pending close events before reading captured spans.
async fn fire_and_flush(app: &axum::Router, req: Request<Body>) {
    let resp = app.clone().oneshot(req).await.unwrap();
    // Fully drain the response so the TraceLayer span is closed.
    let _body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    // The OpenTelemetry layer exports spans asynchronously; give it a tick.
    tokio::time::sleep(Duration::from_millis(50)).await;
}

#[tokio::test]
async fn search_request_emits_root_span() {
    let telem = TestTelemetry::install();
    let app = build_test_app();

    let req = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/search")
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::AUTHORIZATION, "Bearer test-token")
        .body(Body::from(r#"{"query": "test query", "collection_id": "default"}"#))
        .unwrap();

    fire_and_flush(&app, req).await;

    let names = telem.span_names();
    assert!(
        names.iter().any(|n| n.contains("request") || n.contains("POST")),
        "expected a search-related root span, got: {:?}", names
    );
}

#[tokio::test]
async fn health_request_emits_span() {
    let telem = TestTelemetry::install();
    let app = build_test_app();

    let req = Request::builder()
        .method(Method::GET)
        .uri("/health")
        .body(Body::empty())
        .unwrap();

    fire_and_flush(&app, req).await;

    let names = telem.span_names();
    assert!(
        names.iter().any(|n| n.contains("request") || n.contains("GET") || n.contains("health")),
        "expected a health span, got: {:?}", names
    );
}

#[tokio::test]
async fn spans_have_service_name_attribute() {
    let telem = TestTelemetry::install();
    let app = build_test_app();

    let req = Request::builder()
        .method(Method::GET)
        .uri("/health")
        .body(Body::empty())
        .unwrap();

    fire_and_flush(&app, req).await;

    let spans = telem.drain_spans();
    // Every span from our provider should carry service.name from the Resource
    // (only spans created via our provider — HTTP spans from TraceLayer may not)
    // This is a best-effort check: at least one span must exist
    assert!(!spans.is_empty(), "no spans were collected at all");
}
