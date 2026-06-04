use axum::http::{StatusCode, HeaderMap};
use axum::Json;
use arcanum_engine::{ArcanumEngine, auth::ApiKeyClaims};
use std::sync::Arc;

/// Extract and validate a Bearer API key. Returns 401 if absent or invalid.
pub fn validate_bearer(
    headers: &HeaderMap,
    engine: &Option<Arc<ArcanumEngine>>,
) -> Result<ApiKeyClaims, (StatusCode, Json<serde_json::Value>)> {
    let Some(engine) = engine else {
        return Err((StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": "engine not initialised" }))));
    };
    let header_val = headers.get("Authorization")
        .ok_or_else(|| (StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": "missing Authorization header" }))))?;
    let raw = header_val.to_str().unwrap_or("");
    let token = raw.strip_prefix("Bearer ").unwrap_or(raw);
    engine.auth.validate_api_key(token)
        .map_err(|_| (StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": "invalid or expired token" }))))
}
