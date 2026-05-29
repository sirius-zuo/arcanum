use axum::{extract::{State, Json}, http::{StatusCode, HeaderMap}, response::IntoResponse};
use serde::Deserialize;
use std::sync::Arc;
use arcanum_engine::ArcanumEngine;

/// Extract and validate a Bearer token. Returns 401 if absent or invalid.
fn validate_bearer(headers: &HeaderMap, engine: &Option<Arc<ArcanumEngine>>)
    -> Result<arcanum_engine::auth::ApiKeyClaims, (StatusCode, axum::Json<serde_json::Value>)>
{
    let Some(engine) = engine else {
        // No engine in test mode — reject all auth (fail-closed).
        return Err((StatusCode::UNAUTHORIZED,
            axum::Json(serde_json::json!({ "error": "engine not initialised" }))));
    };
    let header_val = headers.get("Authorization")
        .ok_or_else(|| (StatusCode::UNAUTHORIZED,
            axum::Json(serde_json::json!({ "error": "missing Authorization header" }))))?;
    let token = header_val.to_str()
        .unwrap_or("")
        .strip_prefix("Bearer ")
        .unwrap_or(header_val.to_str().unwrap_or(""));
    engine.auth.validate_api_key(token)
        .map_err(|_| (StatusCode::UNAUTHORIZED,
            axum::Json(serde_json::json!({ "error": "invalid or expired token" }))))
}

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
    State(engine): State<Option<Arc<ArcanumEngine>>>,
    Json(req): Json<SearchRequest>,
) -> impl IntoResponse {
    let claims = match validate_bearer(&headers, &engine) {
        Ok(c) => c,
        Err(e) => return e.into_response(),
    };
    // Verify collection access.
    let collection = req.collection_id.as_deref().unwrap_or("");
    if !engine.as_ref().unwrap().auth.can_access_collection(&claims, collection) {
        return (StatusCode::FORBIDDEN,
            axum::Json(serde_json::json!({ "error": "access denied" }))).into_response();
    }
    (StatusCode::OK, axum::Json(serde_json::json!({ "chunks": [], "confidence": 0.0 }))).into_response()
}

pub async fn ingest(
    headers: HeaderMap,
    State(engine): State<Option<Arc<ArcanumEngine>>>,
    Json(req): Json<IngestRequest>,
) -> impl IntoResponse {
    let claims = match validate_bearer(&headers, &engine) {
        Ok(c) => c,
        Err(e) => return e.into_response(),
    };
    if !engine.as_ref().unwrap().auth.can_access_collection(&claims, &req.collection_id) {
        return (StatusCode::FORBIDDEN,
            axum::Json(serde_json::json!({ "error": "access denied" }))).into_response();
    }
    (StatusCode::ACCEPTED, axum::Json(serde_json::json!({
        "operation_id": uuid::Uuid::new_v4().to_string(),
        "status": "queued"
    }))).into_response()
}
