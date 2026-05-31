use axum::{Router, routing::{get, post}, http::Method};
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use std::sync::Arc;
use arcanum_engine::ArcanumEngine;
use crate::routes::{api, health, admin};
use crate::ws::ws_handler;

pub fn build_app(engine: Option<Arc<ArcanumEngine>>) -> Router {
    // In production, configure allowed_origins from ArcanumConfig.
    // Default: deny all cross-origin requests (fail-closed).
    let cors = CorsLayer::new()
        .allow_methods([Method::GET, Method::POST])
        .allow_headers([
            axum::http::header::AUTHORIZATION,
            axum::http::header::CONTENT_TYPE,
        ]);
    // Note: allow_origin is omitted → no origin allowed by default.
    // Production callers must pass explicit origins via config.

    Router::new()
        .route("/health", get(health::liveness))
        .route("/ready",  get(health::readiness))
        .route("/api/v1/search", post(api::search))
        .route("/api/v1/ingest", post(api::ingest))
        .route("/admin/collections", get(admin::list_collections))
        .route("/admin/health",      get(admin::get_health))
        .route("/admin/metrics",     get(admin::get_metrics))
        .route("/admin/sources",     get(admin::list_ingestion_sources))
        .route("/admin/audit",       get(admin::get_audit_logs))
        .route("/admin/rotate-keys", post(admin::rotate_keys))
        .route("/ws/events",         get(ws_handler))
        .layer(TraceLayer::new_for_http())
        .layer(cors)
        .with_state(engine)
}
