use axum::{extract::State, http::{StatusCode, HeaderMap}, response::IntoResponse, Json};
use std::sync::Arc;
use arcanum_engine::ArcanumEngine;

pub async fn list_collections(
    headers: HeaderMap,
    State(_engine): State<Option<Arc<ArcanumEngine>>>,
) -> impl IntoResponse {
    let Some(_auth) = headers.get("Authorization") else {
        return (StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({ "error": "missing admin JWT" }))).into_response();
    };
    (StatusCode::OK, Json(serde_json::json!({ "collections": [] }))).into_response()
}

pub async fn get_health(State(_engine): State<Option<Arc<ArcanumEngine>>>) -> impl IntoResponse {
    (StatusCode::OK, Json(serde_json::json!({ "vector_store": "ok" }))).into_response()
}

pub async fn get_metrics(State(_engine): State<Option<Arc<ArcanumEngine>>>) -> impl IntoResponse {
    (StatusCode::OK, Json(serde_json::json!({ "uptime_secs": 0 }))).into_response()
}
