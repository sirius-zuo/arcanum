use axum::{extract::State, http::{StatusCode, HeaderMap}, response::IntoResponse, Json};
use std::sync::Arc;
use arcanum_engine::ArcanumEngine;
use arcanum_engine::auth::{AdminClaims, AdminRole};
use arcanum_engine::services::admin::AdminService;

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

pub async fn list_collections(
    headers: HeaderMap,
    State(engine): State<Option<Arc<ArcanumEngine>>>,
) -> impl IntoResponse {
    let claims = match validate_admin_bearer(&headers, &engine) {
        Ok(c) => c,
        Err(e) => return e.into_response(),
    };
    if let Err(e) = AdminService::require_role(&claims.role, &AdminRole::Operator) {
        return (StatusCode::FORBIDDEN,
            Json(serde_json::json!({ "error": e.to_string() }))).into_response();
    }
    (StatusCode::OK, Json(serde_json::json!({ "collections": [] }))).into_response()
}

pub async fn get_health(
    headers: HeaderMap,
    State(engine): State<Option<Arc<ArcanumEngine>>>,
) -> impl IntoResponse {
    let claims = match validate_admin_bearer(&headers, &engine) {
        Ok(c) => c,
        Err(e) => return e.into_response(),
    };
    if let Err(e) = AdminService::require_role(&claims.role, &AdminRole::Tester) {
        return (StatusCode::FORBIDDEN,
            Json(serde_json::json!({ "error": e.to_string() }))).into_response();
    }
    (StatusCode::OK, Json(serde_json::json!({ "vector_store": "ok" }))).into_response()
}

pub async fn get_metrics(
    headers: HeaderMap,
    State(engine): State<Option<Arc<ArcanumEngine>>>,
) -> impl IntoResponse {
    let claims = match validate_admin_bearer(&headers, &engine) {
        Ok(c) => c,
        Err(e) => return e.into_response(),
    };
    if let Err(e) = AdminService::require_role(&claims.role, &AdminRole::Tester) {
        return (StatusCode::FORBIDDEN,
            Json(serde_json::json!({ "error": e.to_string() }))).into_response();
    }
    (StatusCode::OK, Json(serde_json::json!({ "uptime_secs": 0 }))).into_response()
}

pub async fn list_ingestion_sources(
    headers: HeaderMap,
    State(engine): State<Option<Arc<ArcanumEngine>>>,
) -> impl IntoResponse {
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
}

pub async fn get_audit_logs(
    headers: HeaderMap,
    State(engine): State<Option<Arc<ArcanumEngine>>>,
) -> impl IntoResponse {
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
}

pub async fn rotate_keys(
    headers: HeaderMap,
    State(engine): State<Option<Arc<ArcanumEngine>>>,
) -> impl IntoResponse {
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
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({ "status": "rotated" }))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() }))).into_response(),
    }
}

#[cfg(test)]
mod tests {
    use axum::{body::Body, http::{Request, StatusCode}};
    use tower::ServiceExt;
    use arcanum_engine::ArcanumEngine;
    use crate::build_app;
    use std::sync::Arc;

    async fn test_engine() -> Arc<ArcanumEngine> {
        ArcanumEngine::builder()
            .auth_secret("a-32-char-secret-for-testing-ok!")
            .build()
            .await
            .expect("engine build")
    }

    async fn get_with_token(path: &str, token: &str) -> StatusCode {
        let engine = test_engine().await;
        let app = build_app(Some(engine));
        let req = Request::builder()
            .uri(path)
            .header("Authorization", format!("Bearer {}", token))
            .body(Body::empty())
            .unwrap();
        app.oneshot(req).await.unwrap().status()
    }

    #[tokio::test]
    async fn test_health_requires_tester_role_minimum() {
        let engine = test_engine().await;
        let token = engine.auth.generate_admin_key("admin1");
        let status = get_with_token("/admin/health", &token).await;
        assert_eq!(status, StatusCode::OK, "admin key should pass health check");
    }

    #[tokio::test]
    async fn test_health_rejects_non_admin_api_key() {
        let engine = test_engine().await;
        let token = engine.auth.generate_api_key("user1", vec!["col1".into()]);
        let status = get_with_token("/admin/health", &token).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "non-admin key should be rejected");
    }

    #[tokio::test]
    async fn test_collections_requires_operator_role_minimum() {
        let engine = test_engine().await;
        let token = engine.auth.generate_admin_key("admin1");
        let status = get_with_token("/admin/collections", &token).await;
        assert_eq!(status, StatusCode::OK, "admin key should pass collections check");
    }

    #[tokio::test]
    async fn test_metrics_rejects_unauthenticated() {
        let engine = test_engine().await;
        let app = build_app(Some(engine));
        let req = Request::builder()
            .uri("/admin/metrics")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }
}
