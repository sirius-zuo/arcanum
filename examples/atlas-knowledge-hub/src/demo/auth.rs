use crate::state::AtlasState;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;

/// Error type shared by all `/demo` routes; renders as `{"error": "..."}`.
#[derive(Debug)]
pub enum DemoError {
    Unauthorized(String),
    Forbidden(String),
    NotFound(String),
    Conflict(String),
    Internal(String),
    Unavailable(String),
}

impl IntoResponse for DemoError {
    fn into_response(self) -> Response {
        let (status, msg) = match self {
            DemoError::Unauthorized(m) => (StatusCode::UNAUTHORIZED, m),
            DemoError::Forbidden(m) => (StatusCode::FORBIDDEN, m),
            DemoError::NotFound(m) => (StatusCode::NOT_FOUND, m),
            DemoError::Conflict(m) => (StatusCode::CONFLICT, m),
            DemoError::Internal(m) => (StatusCode::INTERNAL_SERVER_ERROR, m),
            DemoError::Unavailable(m) => (StatusCode::SERVICE_UNAVAILABLE, m),
        };
        (status, Json(serde_json::json!({ "error": msg }))).into_response()
    }
}

/// Validates `Authorization: Bearer <key>` with the engine's auth, like the real API
/// (a bare token is accepted too).
pub fn require_key(state: &AtlasState, headers: &HeaderMap) -> Result<(), DemoError> {
    let raw = headers
        .get("Authorization")
        .ok_or_else(|| DemoError::Unauthorized("missing Authorization header".into()))?
        .to_str()
        .unwrap_or("");
    let token = raw.strip_prefix("Bearer ").unwrap_or(raw);
    state
        .engine
        .auth
        .validate_api_key(token)
        .map(|_| ())
        .map_err(|_| DemoError::Unauthorized("invalid or expired token".into()))
}
