use axum::{Router, routing::{get, post}, http::Method};
use arcanum_core::config::ArcanumConfig;
use tower_http::{cors::{CorsLayer, AllowOrigin}, trace::TraceLayer};
use std::sync::Arc;
use arcanum_engine::ArcanumEngine;
use crate::routes::{api, health, admin, graph};
use crate::ws::ws_handler;
use crate::portal::serve_portal;
use crate::metrics;

pub fn build_app_with_config(engine: Option<Arc<ArcanumEngine>>, config: ArcanumConfig) -> Router {
    metrics::init_metrics();
    let origins = &config.server.cors_allowed_origins;
    let cors = {
        let base = CorsLayer::new()
            .allow_methods([Method::GET, Method::POST])
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
        .route("/api/v1/search", post(api::search))
        .route("/api/v1/ingest", post(api::ingest))
        .route("/api/v1/graph",  get(graph::get_graph))
        .route("/api/v1/upload", post(api::upload))
        .route("/admin/collections", get(admin::list_collections))
        .route("/admin/health",      get(admin::get_health))
        .route("/admin/metrics",     get(admin::get_metrics))
        .route("/admin/sources",     get(admin::list_ingestion_sources))
        .route("/admin/audit",       get(admin::get_audit_logs))
        .route("/admin/rotate-keys", post(admin::rotate_keys))
        .route("/admin/ui",          get(serve_portal))
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
    use axum::{body::Body, http::{Request, Method}};
    use tower::ServiceExt;

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
}
