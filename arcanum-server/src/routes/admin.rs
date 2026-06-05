use axum::{extract::State, http::{StatusCode, HeaderMap}, response::IntoResponse, Json};
use std::sync::Arc;
use arcanum_engine::ArcanumEngine;
use arcanum_engine::auth::{AdminClaims, AdminRole};
use arcanum_engine::services::admin::AdminService;
use metrics::counter;

/// Validate RS256 admin JWT and return AdminClaims.
fn validate_admin_bearer(headers: &HeaderMap, engine: &Option<Arc<ArcanumEngine>>)
    -> Result<AdminClaims, (StatusCode, Json<serde_json::Value>)>
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
    // Try RS256 admin JWT first; fall back to API key with admin flag.
    if let Ok(claims) = engine.auth.validate_admin_jwt(token) {
        return Ok(claims);
    }
    // Fall back to HMAC API key with is_admin=true.
    let api_claims = engine.auth.validate_api_key(token)
        .map_err(|_| (StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": "invalid or expired token" }))))?;
    if !api_claims.is_admin {
        return Err((StatusCode::FORBIDDEN,
            Json(serde_json::json!({ "error": "admin access required" }))));
    }
    Ok(AdminClaims {
        sub: api_claims.user_id,
        role: AdminRole::Admin,
        exp: api_claims.exp as u64,
    })
}

pub async fn list_ingestion_sources(
    headers: HeaderMap,
    State(engine): State<Option<Arc<ArcanumEngine>>>,
) -> impl IntoResponse {
    let response = {
        let claims = match validate_admin_bearer(&headers, &engine) {
            Ok(c) => c,
            Err(e) => return e.into_response(),
        };
        if let Err(e) = AdminService::require_role(&claims.role, &AdminRole::Operator) {
            return (StatusCode::FORBIDDEN,
                Json(serde_json::json!({ "error": e.to_string() }))).into_response();
        }
        let eng = engine.as_ref().unwrap();
        let sources = eng.source.list().await;
        (StatusCode::OK, Json(serde_json::json!({ "sources": sources }))).into_response()
    };
    let status = if response.status() == StatusCode::OK { "ok" } else { "error" };
    counter!("arcanum_requests_total", "endpoint" => "admin/list_ingestion_sources", "status" => status).increment(1);
    response
}

pub async fn get_audit_logs(
    headers: HeaderMap,
    State(engine): State<Option<Arc<ArcanumEngine>>>,
) -> impl IntoResponse {
    let response = {
        let claims = match validate_admin_bearer(&headers, &engine) {
            Ok(c) => c,
            Err(e) => return e.into_response(),
        };
        if let Err(e) = AdminService::require_role(&claims.role, &AdminRole::Operator) {
            return (StatusCode::FORBIDDEN,
                Json(serde_json::json!({ "error": e.to_string() }))).into_response();
        }
        let eng = engine.as_ref().unwrap();
        let logs = eng.audit.query(100).await;
        (StatusCode::OK, Json(serde_json::json!({ "logs": logs }))).into_response()
    };
    let status = if response.status() == StatusCode::OK { "ok" } else { "error" };
    counter!("arcanum_requests_total", "endpoint" => "admin/get_audit_logs", "status" => status).increment(1);
    response
}

pub async fn rotate_keys(
    headers: HeaderMap,
    State(engine): State<Option<Arc<ArcanumEngine>>>,
) -> impl IntoResponse {
    let response = {
        let claims = match validate_admin_bearer(&headers, &engine) {
            Ok(c) => c,
            Err(e) => return e.into_response(),
        };
        if let Err(e) = AdminService::require_role(&claims.role, &AdminRole::Admin) {
            return (StatusCode::FORBIDDEN,
                Json(serde_json::json!({ "error": e.to_string() }))).into_response();
        }
        let eng = engine.as_ref().unwrap();
        match eng.admin.rotate_keys(&claims.sub).await {
            Ok(()) => {
                if let Some(store) = &eng.secret_store {
                    if let Err(e) = store.reload().await {
                        tracing::warn!("SecretStore reload after rotate_keys failed: {}", e);
                    }
                }
                (StatusCode::OK, Json(serde_json::json!({ "status": "rotated" }))).into_response()
            }
            Err(e) => (StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() }))).into_response(),
        }
    };
    let status = if response.status() == StatusCode::OK { "ok" } else { "error" };
    counter!("arcanum_requests_total", "endpoint" => "admin/rotate_keys", "status" => status).increment(1);
    response
}

