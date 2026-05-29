use axum::{extract::{State, Json}, http::{StatusCode, HeaderMap}, response::IntoResponse};
use serde::Deserialize;
use std::sync::Arc;
use arcanum_engine::ArcanumEngine;

#[derive(Deserialize)]
pub struct SearchRequest {
    pub query: String,
    pub collection_id: Option<String>,
    pub top_k: Option<usize>,
}

#[derive(Deserialize)]
pub struct IngestRequest {
    pub source_uri: String,
    pub collection_id: String,
    pub pipeline: Option<String>,
}

pub async fn search(
    headers: HeaderMap,
    State(_engine): State<Option<Arc<ArcanumEngine>>>,
    Json(_req): Json<SearchRequest>,
) -> impl IntoResponse {
    let Some(_token) = headers.get("Authorization") else {
        return (StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({ "error": "missing Authorization header" }))).into_response();
    };
    (StatusCode::OK, Json(serde_json::json!({ "chunks": [], "confidence": 0.0 }))).into_response()
}

pub async fn ingest(
    headers: HeaderMap,
    State(_engine): State<Option<Arc<ArcanumEngine>>>,
    Json(_req): Json<IngestRequest>,
) -> impl IntoResponse {
    let Some(_token) = headers.get("Authorization") else {
        return (StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({ "error": "missing Authorization header" }))).into_response();
    };
    (StatusCode::ACCEPTED, Json(serde_json::json!({
        "operation_id": uuid::Uuid::new_v4().to_string(),
        "status": "queued"
    }))).into_response()
}
