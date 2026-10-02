use axum::{http::StatusCode, response::IntoResponse, Json};

pub async fn liveness() -> impl IntoResponse {
    (
        StatusCode::OK,
        Json(serde_json::json!({ "status": "alive" })),
    )
}

pub async fn readiness() -> impl IntoResponse {
    (
        StatusCode::OK,
        Json(serde_json::json!({ "status": "ready" })),
    )
}
