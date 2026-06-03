use axum::{
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};

/// Serve current metrics in Prometheus text exposition format.
///
/// If `ARCANUM_METRICS_TOKEN` is set, requires `Authorization: Bearer <token>`.
/// Returns 401 when the token is missing or wrong.
/// Returns 200 with `Content-Type: text/plain; version=0.0.4` on success.
pub async fn get_metrics(headers: HeaderMap) -> Response {
    if let Ok(expected) = std::env::var("ARCANUM_METRICS_TOKEN") {
        let provided = headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "));
        if provided != Some(expected.as_str()) {
            return StatusCode::UNAUTHORIZED.into_response();
        }
    }

    let text = crate::metrics::get_metrics_text();
    (
        StatusCode::OK,
        [(axum::http::header::CONTENT_TYPE, "text/plain; version=0.0.4")],
        text,
    )
        .into_response()
}
