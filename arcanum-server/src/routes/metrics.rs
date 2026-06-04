use axum::{
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};

/// Serve current metrics in Prometheus text exposition format.
///
/// `ARCANUM_METRICS_TOKEN` is required. Returns 500 if not set, 401 on bad token.
pub async fn get_metrics(headers: HeaderMap) -> Response {
    let expected = match std::env::var("ARCANUM_METRICS_TOKEN") {
        Ok(t) => t,
        Err(_) => {
            return (StatusCode::INTERNAL_SERVER_ERROR,
                "ARCANUM_METRICS_TOKEN is not set".to_string()).into_response();
        }
    };
    let provided = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));
    if provided != Some(expected.as_str()) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let text = crate::metrics::get_metrics_text();
    (
        StatusCode::OK,
        [(axum::http::header::CONTENT_TYPE, "text/plain; version=0.0.4")],
        text,
    )
        .into_response()
}
