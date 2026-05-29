use axum::{extract::State, http::{StatusCode, HeaderMap}, response::IntoResponse, Json};
use std::sync::Arc;
use arcanum_engine::ArcanumEngine;

/// Validate admin JWT — returns 401 on missing/invalid, 403 on non-admin.
fn require_admin(headers: &HeaderMap, engine: &Option<Arc<ArcanumEngine>>)
    -> Result<(), (StatusCode, Json<serde_json::Value>)>
{
    let Some(engine) = engine else {
        return Err((StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": "engine not initialised" }))));
    };
    let header_val = headers.get("Authorization")
        .ok_or_else(|| (StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": "missing Authorization header" }))))?;
    let token = header_val.to_str()
        .unwrap_or("")
        .strip_prefix("Bearer ")
        .unwrap_or(header_val.to_str().unwrap_or(""));
    let claims = engine.auth.validate_api_key(token)
        .map_err(|_| (StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": "invalid or expired token" }))))?;
    if !claims.is_admin {
        return Err((StatusCode::FORBIDDEN,
            Json(serde_json::json!({ "error": "admin access required" }))));
    }
    Ok(())
}

pub async fn list_collections(
    headers: HeaderMap,
    State(engine): State<Option<Arc<ArcanumEngine>>>,
) -> impl IntoResponse {
    if let Err(e) = require_admin(&headers, &engine) { return e.into_response(); }
    (StatusCode::OK, Json(serde_json::json!({ "collections": [] }))).into_response()
}

pub async fn get_health(
    headers: HeaderMap,
    State(engine): State<Option<Arc<ArcanumEngine>>>,
) -> impl IntoResponse {
    if let Err(e) = require_admin(&headers, &engine) { return e.into_response(); }
    (StatusCode::OK, Json(serde_json::json!({ "vector_store": "ok" }))).into_response()
}

pub async fn get_metrics(
    headers: HeaderMap,
    State(engine): State<Option<Arc<ArcanumEngine>>>,
) -> impl IntoResponse {
    if let Err(e) = require_admin(&headers, &engine) { return e.into_response(); }
    (StatusCode::OK, Json(serde_json::json!({ "uptime_secs": 0 }))).into_response()
}
