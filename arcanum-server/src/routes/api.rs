use axum::{extract::{State, Json}, http::{StatusCode, HeaderMap}, response::{IntoResponse, Response}};
use serde::Deserialize;
use std::sync::Arc;
use arcanum_core::types::{Query, CollectionId};
use arcanum_engine::{ArcanumEngine, auth::ApiKeyClaims};
use metrics::{counter, histogram};


/// Extract and validate a Bearer token. Returns 401 if absent or invalid.
fn validate_bearer(headers: &HeaderMap, engine: &Option<Arc<ArcanumEngine>>)
    -> Result<ApiKeyClaims, (StatusCode, axum::Json<serde_json::Value>)>
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
pub struct HttpIngestRequest {
    pub source_uri: String,
    pub collection_id: String,
    pub pipeline: Option<String>,
    pub force: Option<bool>,
}

#[tracing::instrument(skip_all)]
pub async fn search(
    headers: HeaderMap,
    State(engine): State<Option<Arc<ArcanumEngine>>>,
    Json(req): Json<SearchRequest>,
) -> impl IntoResponse {
    let start = std::time::Instant::now();
    let response: Response = {
        let claims = match validate_bearer(&headers, &engine) {
            Ok(c) => c,
            Err(e) => return e.into_response(),
        };
        let eng = engine.as_ref().unwrap();
        let collection = req.collection_id.as_deref().unwrap_or("");
        if !eng.auth.can_access_collection(&claims, collection) {
            return (StatusCode::FORBIDDEN,
                axum::Json(serde_json::json!({ "error": "access denied" }))).into_response();
        }

        let query = Query::new(&req.query)
            .with_collection(CollectionId(collection.to_string()))
            .with_top_k(req.top_k.unwrap_or(10));

        match eng.retrieval.search(query, &claims).await {
            Ok(result) => (StatusCode::OK, axum::Json(serde_json::json!({
                "chunks": result.chunks,
                "confidence": result.confidence,
                "strategy_scores": result.strategy_scores,
            }))).into_response(),
            Err(e) => (StatusCode::INTERNAL_SERVER_ERROR,
                axum::Json(serde_json::json!({ "error": e.to_string() }))).into_response(),
        }
    };
    let elapsed = start.elapsed().as_secs_f64();
    let status = if response.status() == StatusCode::OK { "ok" } else { "error" };
    counter!("arcanum_requests_total", "endpoint" => "search", "status" => status).increment(1);
    histogram!("arcanum_request_duration_seconds", "endpoint" => "search").record(elapsed);
    response
}

#[tracing::instrument(skip_all)]
pub async fn ingest(
    headers: HeaderMap,
    State(engine): State<Option<Arc<ArcanumEngine>>>,
    Json(req): Json<HttpIngestRequest>,
) -> impl IntoResponse {
    let start = std::time::Instant::now();
    let response: Response = {
        let claims = match validate_bearer(&headers, &engine) {
            Ok(c) => c,
            Err(e) => return e.into_response(),
        };
        let eng = engine.as_ref().unwrap();
        if !eng.auth.can_access_collection(&claims, &req.collection_id) {
            return (StatusCode::FORBIDDEN,
                axum::Json(serde_json::json!({ "error": "access denied" }))).into_response();
        }

        let ingest_req = arcanum_engine::IngestRequest {
            source_uri: req.source_uri,
            collection_id: CollectionId(req.collection_id),
            pipeline_template: req.pipeline,
            force: req.force.unwrap_or(false),
            content: None,
            mime_hint: None,
        };

        match eng.ingestion.ingest(ingest_req, &claims.user_id).await {
            Ok(op_id) => (StatusCode::ACCEPTED, axum::Json(serde_json::json!({
                "operation_id": op_id.0,
                "status": "queued"
            }))).into_response(),
            Err(e) => (StatusCode::INTERNAL_SERVER_ERROR,
                axum::Json(serde_json::json!({ "error": e.to_string() }))).into_response(),
        }
    };
    let elapsed = start.elapsed().as_secs_f64();
    let status = if response.status() == StatusCode::ACCEPTED { "ok" } else { "error" };
    counter!("arcanum_requests_total", "endpoint" => "ingest", "status" => status).increment(1);
    histogram!("arcanum_request_duration_seconds", "endpoint" => "ingest").record(elapsed);
    response
}

#[derive(Deserialize)]
pub struct UploadParams {
    pub collection_id: String,
    pub filename: String,
    pub pipeline: Option<String>,
    pub force: Option<bool>,
}

/// Best-effort MIME hint from a filename extension.
fn mime_from_filename(name: &str) -> String {
    let ext = name.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        "md" | "markdown" => "text/markdown",
        "html" | "htm"    => "text/html",
        "txt"             => "text/plain",
        "pdf"             => "application/pdf",
        "epub"            => "application/epub+zip",
        "docx"            => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        _                 => "application/octet-stream",
    }.to_string()
}

/// POST /api/v1/upload?collection_id=X&filename=foo.md&pipeline=full
/// Body: raw file bytes. Ingests the bytes inline via Source::Raw.
pub async fn upload(
    headers: HeaderMap,
    State(engine): State<Option<Arc<ArcanumEngine>>>,
    axum::extract::Query(params): axum::extract::Query<UploadParams>,
    body: axum::body::Bytes,
) -> impl IntoResponse {
    let start = std::time::Instant::now();
    let response: Response = {
        let claims = match validate_bearer(&headers, &engine) {
            Ok(c) => c,
            Err(e) => return e.into_response(),
        };
        let eng = engine.as_ref().unwrap();
        if !eng.auth.can_access_collection(&claims, &params.collection_id) {
            return (StatusCode::FORBIDDEN,
                axum::Json(serde_json::json!({ "error": "access denied" }))).into_response();
        }

        let ingest_req = arcanum_engine::IngestRequest {
            source_uri: params.filename.clone(),
            collection_id: CollectionId(params.collection_id),
            pipeline_template: params.pipeline,
            force: params.force.unwrap_or(false),
            content: Some(body.to_vec()),
            mime_hint: Some(mime_from_filename(&params.filename)),
        };

        match eng.ingestion.ingest(ingest_req, &claims.user_id).await {
            Ok(op_id) => (StatusCode::ACCEPTED, axum::Json(serde_json::json!({
                "operation_id": op_id.0,
                "status": "queued"
            }))).into_response(),
            Err(e) => (StatusCode::INTERNAL_SERVER_ERROR,
                axum::Json(serde_json::json!({ "error": e.to_string() }))).into_response(),
        }
    };
    let elapsed = start.elapsed().as_secs_f64();
    let status = if response.status() == StatusCode::ACCEPTED { "ok" } else { "error" };
    counter!("arcanum_requests_total", "endpoint" => "upload", "status" => status).increment(1);
    histogram!("arcanum_request_duration_seconds", "endpoint" => "upload").record(elapsed);
    response
}

#[cfg(test)]
mod upload_tests {
    use crate::build_app;
    use axum::body::Body;
    use axum::http::{Request, StatusCode, Method};
    use tower::ServiceExt;

    #[tokio::test]
    async fn upload_requires_auth() {
        let app = build_app(None);
        let resp = app.oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/upload?collection_id=c&filename=f.md")
                .body(Body::from("hello"))
                .unwrap()
        ).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn upload_force_param_is_accepted() {
        use arcanum_engine::ArcanumEngine;
        let engine = ArcanumEngine::builder()
            .auth_secret("a-32-char-secret-for-testing-ok!")
            .build()
            .await
            .unwrap();
        let token = engine.auth.generate_admin_key("tester");
        let app = crate::build_app(Some(engine));
        let resp = app.oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/upload?collection_id=c&filename=f.md&force=true")
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::from(b"# Hello\nworld".to_vec()))
                .unwrap()
        ).await.unwrap();
        assert_eq!(resp.status(), StatusCode::ACCEPTED);
    }
}
