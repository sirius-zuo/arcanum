// tests/integration/telemetry_smoke_test.rs
//
// End-to-end telemetry smoke test. Uses arcanum_telemetry::testing::TestTelemetry
// to capture spans in memory, fires real HTTP requests against an in-process
// server, and asserts the expected span names exist.

use arcanum_telemetry::testing::TestTelemetry;
use axum::{
    body::{Body, to_bytes},
    http::{Request, Method, header},
};
use serial_test::serial;
use tower::ServiceExt;

fn build_test_app() -> axum::Router {
    arcanum_server::build_app(None)
}

/// Send a request and fully consume the response body so the TraceLayer
/// span is closed before we read captured spans.
/// SimpleSpanProcessor exports synchronously — no sleep needed.
async fn fire_and_flush(app: &axum::Router, req: Request<Body>) {
    let resp = app.clone().oneshot(req).await.unwrap();
    let _ = to_bytes(resp.into_body(), 1024 * 1024).await.unwrap();
}

#[tokio::test]
#[serial]
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

    let spans = telem.get_spans();
    let names = telem.span_names();
    assert!(
        spans.iter().any(|s| s.name == "search"),
        "expected a 'search' span from #[tracing::instrument] on the handler, got: {:?}", names
    );
}

#[tokio::test]
#[serial]
async fn ingest_request_emits_span() {
    let telem = TestTelemetry::install();
    let app = build_test_app();

    // Engine is None → validate_bearer returns 401, but #[tracing::instrument]
    // creates the "ingest" span before any early return executes.
    let req = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/ingest")
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::AUTHORIZATION, "Bearer test-token")
        .body(Body::from(
            r#"{"source_uri": "https://example.com/doc.pdf", "collection_id": "default"}"#,
        ))
        .unwrap();

    fire_and_flush(&app, req).await;

    let spans = telem.get_spans();
    let names = telem.span_names();
    assert!(
        spans.iter().any(|s| s.name == "ingest"),
        "expected an 'ingest' span from #[tracing::instrument] on the handler, got: {:?}", names
    );
}

#[tokio::test]
#[serial]
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
    // The health handler has no #[tracing::instrument]; only the TraceLayer
    // "request" span is emitted for this route.
    assert!(
        names.iter().any(|n| n.contains("request") || n.contains("GET") || n.contains("health")),
        "expected a health-request span, got: {:?}", names
    );
}

#[tokio::test]
#[serial]
async fn health_request_produces_spans() {
    let telem = TestTelemetry::install();
    let app = build_test_app();

    let req = Request::builder()
        .method(Method::GET)
        .uri("/health")
        .body(Body::empty())
        .unwrap();

    fire_and_flush(&app, req).await;

    let spans = telem.get_spans();
    assert!(
        !spans.is_empty(),
        "expected at least one span to be emitted for a health request"
    );
}
