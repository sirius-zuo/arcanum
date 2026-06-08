use axum::{Router, routing::{get, post}, http::Method};
use arcanum_core::config::ArcanumConfig;
use tower_http::{cors::{CorsLayer, AllowOrigin}, trace::TraceLayer};
use std::sync::Arc;
use arcanum_engine::ArcanumEngine;
use crate::routes::{api, health, admin, graph, collections, experiments};
use crate::routes::metrics as route_metrics;
use crate::ws::ws_handler;
use crate::portal::serve_portal;

pub fn build_app_with_config(engine: Option<Arc<ArcanumEngine>>, config: ArcanumConfig) -> Router {
    // Recorder installation is owned by arcanum_telemetry::init().
    // Calling init_metrics() here created a dual-init race; removed.
    let origins = &config.server.cors_allowed_origins;
    let cors = {
        let base = CorsLayer::new()
            .allow_methods([Method::GET, Method::POST, Method::DELETE])
            .allow_headers([
                axum::http::header::AUTHORIZATION,
                axum::http::header::CONTENT_TYPE,
            ]);
        if origins.is_empty() {
            base
        } else {
            let allowed: Vec<axum::http::HeaderValue> = origins.iter()
                .filter_map(|o| o.parse().ok())
                .collect();
            base.allow_origin(AllowOrigin::list(allowed))
        }
    };

    Router::new()
        .route("/health", get(health::liveness))
        .route("/ready",  get(health::readiness))
        .route("/metrics", get(route_metrics::get_metrics))
        .route("/api/v1/search", post(api::search))
        .route("/api/v1/ingest",    post(api::ingest))
        .route("/api/v1/graph",     get(graph::get_graph))
        // Chunk eval
        .route("/api/v1/chunk/inspect",    post(api::chunk_inspect))
        .route("/api/v1/chunk/benchmark",  post(api::chunk_benchmark))
        .route("/api/v1/upload", post(api::upload))
        // Shadow experiments
        .route("/api/v1/collections/:collection_id/experiments",
            axum::routing::post(experiments::start_experiment))
        .route("/api/v1/collections/:collection_id/experiments/:experiment_id",
            axum::routing::get(experiments::get_experiment))
        .route("/api/v1/collections/:collection_id/experiments/:experiment_id/promote",
            axum::routing::post(experiments::promote_experiment))
        .route("/api/v1/collections/:collection_id/experiments/:experiment_id",
            axum::routing::delete(experiments::abandon_experiment))
        .route("/admin/sources",     get(admin::list_ingestion_sources))
        .route("/admin/audit",       get(admin::get_audit_logs))
        .route("/admin/rotate-keys", post(admin::rotate_keys))
        .route("/admin/ui",          get(serve_portal))
        // Vector collections
        .route("/api/v1/vector/collections",              get(collections::vector_list))
        .route("/api/v1/vector/collections/stats",        get(collections::vector_stats_all))
        .route("/api/v1/vector/collections/:name",        post(collections::vector_create).delete(collections::vector_delete))
        .route("/api/v1/vector/collections/:name/stats",  get(collections::vector_stats_one))
        .route("/api/v1/vector/collections/:name/documents", get(collections::vector_list_documents).delete(collections::vector_delete_document))
        // Graph collections (stubs)
        .route("/api/v1/graph/collections",               get(collections::graph_list))
        .route("/api/v1/graph/collections/stats",         get(collections::graph_stats_all))
        .route("/api/v1/graph/collections/:name",         post(collections::graph_create).delete(collections::graph_delete))
        .route("/api/v1/graph/collections/:name/stats",   get(collections::graph_stats_one))
        // Tree collections
        .route("/api/v1/tree/collections",                get(collections::tree_list))
        .route("/api/v1/tree/collections/stats",          get(collections::tree_stats_all))
        .route("/api/v1/tree/collections/:name",          post(collections::tree_create).delete(collections::tree_delete))
        .route("/api/v1/tree/collections/:name/stats",    get(collections::tree_stats_one))
        .route("/ws/events",         get(ws_handler))
        .layer(TraceLayer::new_for_http())
        .layer(cors)
        .with_state(engine)
}

pub fn build_app(engine: Option<Arc<ArcanumEngine>>) -> Router {
    build_app_with_config(engine, ArcanumConfig::default())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::{Request, Method, StatusCode}};
    use tower::ServiceExt;
    use serial_test::serial;

    #[tokio::test]
    async fn test_cors_absent_when_no_origins_configured() {
        let app = build_app(None);
        let req = Request::builder()
            .method(Method::OPTIONS)
            .uri("/health")
            .header("Origin", "https://attacker.example.com")
            .header("Access-Control-Request-Method", "GET")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert!(
            resp.headers().get("access-control-allow-origin").is_none(),
            "no origins configured → no allow-origin header"
        );
    }

    #[tokio::test]
    async fn test_cors_present_when_origin_configured() {
        let mut config = ArcanumConfig::default();
        config.server.cors_allowed_origins = vec!["https://app.example.com".to_string()];
        let app = build_app_with_config(None, config);
        let req = Request::builder()
            .method(Method::OPTIONS)
            .uri("/health")
            .header("Origin", "https://app.example.com")
            .header("Access-Control-Request-Method", "GET")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        let allow_origin = resp.headers()
            .get("access-control-allow-origin")
            .and_then(|v| v.to_str().ok());
        assert_eq!(allow_origin, Some("https://app.example.com"),
            "configured origin should appear in allow-origin header");
    }

    #[tokio::test]
    #[serial]
    async fn test_metrics_endpoint_returns_500_without_token() {
        // ARCANUM_METRICS_TOKEN is now required.
        std::env::remove_var("ARCANUM_METRICS_TOKEN");
        let app = build_app(None);
        let req = Request::builder()
            .uri("/metrics")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR,
            "GET /metrics with no token env var should return 500");
    }

    #[tokio::test]
    #[serial]
    async fn test_metrics_endpoint_returns_401_when_token_set_and_not_provided() {
        std::env::set_var("ARCANUM_METRICS_TOKEN", "test-secret");
        let app = build_app(None);
        let req = Request::builder()
            .uri("/metrics")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        std::env::remove_var("ARCANUM_METRICS_TOKEN");
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED,
            "GET /metrics with token set but not provided should return 401");
    }
}
