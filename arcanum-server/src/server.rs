use axum::{Router, routing::{get, post}};
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use std::sync::Arc;
use arcanum_engine::ArcanumEngine;
use crate::routes::{api, health, admin};
use crate::ws::ws_handler;

pub fn build_app(engine: Option<Arc<ArcanumEngine>>) -> Router {
    Router::new()
        .route("/health", get(health::liveness))
        .route("/ready",  get(health::readiness))
        .route("/api/v1/search", post(api::search))
        .route("/api/v1/ingest", post(api::ingest))
        .route("/admin/collections", get(admin::list_collections))
        .route("/admin/health",      get(admin::get_health))
        .route("/admin/metrics",     get(admin::get_metrics))
        .route("/ws/events",         get(ws_handler))
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
        .with_state(engine)
}
