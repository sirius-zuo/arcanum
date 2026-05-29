use axum::{response::IntoResponse, http::StatusCode, Json};

pub async fn liveness() -> impl IntoResponse {
    (StatusCode::OK, Json(serde_json::json!({ "status": "alive" })))
}

pub async fn readiness() -> impl IntoResponse {
    (StatusCode::OK, Json(serde_json::json!({ "status": "ready" })))
}
