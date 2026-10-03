use crate::routes::auth::validate_bearer;
use arcanum_chunk_eval::{inspect, run_benchmark, BenchmarkJob, InspectRequest};
use arcanum_core::types::{CollectionId, ContextRequest, IngestionSubmission, OperationId, Query};
use arcanum_core::ArcanumError;
use arcanum_engine::services::context::ContextError;
use arcanum_engine::ArcanumEngine;
use axum::{
    extract::{Json, Multipart, Path, Query as UrlQuery, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use metrics::{counter, histogram};
use serde::Deserialize;
use std::sync::Arc;
use uuid::Uuid;

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
            return (
                StatusCode::FORBIDDEN,
                axum::Json(serde_json::json!({ "error": "access denied" })),
            )
                .into_response();
        }

        let query = Query::new(&req.query)
            .with_collection(CollectionId(collection.to_string()))
            .with_top_k(req.top_k.unwrap_or(10));

        match eng.retrieval.search(query, &claims).await {
            Ok(result) => (
                StatusCode::OK,
                axum::Json(serde_json::json!({
                    "chunks": result.chunks,
                    "confidence": result.confidence,
                    "strategy_scores": result.strategy_scores,
                })),
            )
                .into_response(),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                axum::Json(serde_json::json!({ "error": e.to_string() })),
            )
                .into_response(),
        }
    };
    let elapsed = start.elapsed().as_secs_f64();
    let status = if response.status() == StatusCode::OK {
        "ok"
    } else {
        "error"
    };
    counter!("arcanum_requests_total", "endpoint" => "search", "status" => status).increment(1);
    histogram!("arcanum_request_duration_seconds", "endpoint" => "search").record(elapsed);
    response
}

fn context_error_status(e: &ContextError) -> StatusCode {
    match e {
        ContextError::Invalid(_) => StatusCode::BAD_REQUEST,
        ContextError::Forbidden(_) => StatusCode::FORBIDDEN,
        ContextError::Unavailable(_) => StatusCode::SERVICE_UNAVAILABLE,
        ContextError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

#[tracing::instrument(skip_all)]
pub async fn context(
    headers: HeaderMap,
    State(engine): State<Option<Arc<ArcanumEngine>>>,
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    let start = std::time::Instant::now();
    let response: Response = {
        let claims = match validate_bearer(&headers, &engine) {
            Ok(c) => c,
            Err(e) => return e.into_response(),
        };
        let eng = engine.as_ref().unwrap();
        match serde_json::from_value::<ContextRequest>(body) {
            Err(e) => (
                StatusCode::BAD_REQUEST,
                axum::Json(serde_json::json!({ "error": e.to_string() })),
            )
                .into_response(),
            Ok(req) => match eng.context.as_ref() {
                None => (
                    StatusCode::SERVICE_UNAVAILABLE,
                    axum::Json(serde_json::json!({ "error": "context requires a chunk registry" })),
                )
                    .into_response(),
                Some(svc) => match svc.assemble(req, &claims).await {
                    Ok(resp) => (StatusCode::OK, axum::Json(resp)).into_response(),
                    Err(e) => (
                        context_error_status(&e),
                        axum::Json(serde_json::json!({ "error": e.to_string() })),
                    )
                        .into_response(),
                },
            },
        }
    };
    let elapsed = start.elapsed().as_secs_f64();
    let status = if response.status() == StatusCode::OK {
        "ok"
    } else {
        "error"
    };
    counter!("arcanum_requests_total", "endpoint" => "context", "status" => status).increment(1);
    histogram!("arcanum_request_duration_seconds", "endpoint" => "context").record(elapsed);
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
            return (
                StatusCode::FORBIDDEN,
                axum::Json(serde_json::json!({ "error": "access denied" })),
            )
                .into_response();
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
            Ok(op_id) => (
                StatusCode::ACCEPTED,
                axum::Json(serde_json::json!({
                    "operation_id": op_id.0,
                    "status": "queued"
                })),
            )
                .into_response(),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                axum::Json(serde_json::json!({ "error": e.to_string() })),
            )
                .into_response(),
        }
    };
    let elapsed = start.elapsed().as_secs_f64();
    let status = if response.status() == StatusCode::ACCEPTED {
        "ok"
    } else {
        "error"
    };
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
        "html" | "htm" => "text/html",
        "txt" => "text/plain",
        "pdf" => "application/pdf",
        "epub" => "application/epub+zip",
        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        _ => "application/octet-stream",
    }
    .to_string()
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
        if !eng
            .auth
            .can_access_collection(&claims, &params.collection_id)
        {
            return (
                StatusCode::FORBIDDEN,
                axum::Json(serde_json::json!({ "error": "access denied" })),
            )
                .into_response();
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
            Ok(op_id) => (
                StatusCode::ACCEPTED,
                axum::Json(serde_json::json!({
                    "operation_id": op_id.0,
                    "status": "queued"
                })),
            )
                .into_response(),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                axum::Json(serde_json::json!({ "error": e.to_string() })),
            )
                .into_response(),
        }
    };
    let elapsed = start.elapsed().as_secs_f64();
    let status = if response.status() == StatusCode::ACCEPTED {
        "ok"
    } else {
        "error"
    };
    counter!("arcanum_requests_total", "endpoint" => "upload", "status" => status).increment(1);
    histogram!("arcanum_request_duration_seconds", "endpoint" => "upload").record(elapsed);
    response
}

/// DELETE /api/v1/collections/{collectionId}/sources?source_uri=<encoded>
#[derive(Deserialize)]
pub struct DeleteSourceParams {
    pub source_uri: String,
}

/// GET /api/v1/ingestion-operations?idempotency_key=KEY — a single required
/// query value. serde rejects a missing field AND a duplicate field, so the
/// "exactly one query value" rule is enforced at deserialization.
#[derive(Deserialize)]
pub struct IdempotencyQuery {
    pub idempotency_key: String,
}

fn bad_request(msg: impl Into<String>) -> Response {
    (
        StatusCode::BAD_REQUEST,
        Json(serde_json::json!({ "error": msg.into() })),
    )
        .into_response()
}

/// Redacted error mapping for the durable operation routes: only safe codes are
/// surfaced — never internal stack or connection details.
fn operation_error_response(e: &ArcanumError) -> Response {
    match e {
        ArcanumError::Conflict(_) => (StatusCode::CONFLICT,
            Json(serde_json::json!({ "error": "idempotency key already used by a different submission" }))).into_response(),
        ArcanumError::NotFound(_) => (StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "operation not found" }))).into_response(),
        ArcanumError::QueueFull => (StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({ "error": "ingestion queue is full" }))).into_response(),
        ArcanumError::Generation(_) => (StatusCode::BAD_GATEWAY,
            Json(serde_json::json!({ "error": "generation failed" }))).into_response(),
        _ => (StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": "internal error" }))).into_response(),
    }
}

/// Parse a multipart submission body into an `IngestionSubmission`.
///
/// Accepted shapes (the submission type documents payload XOR
/// `payload_locator`):
/// - `metadata` JSON part + `payload` binary part — inline bytes staged by the
///   ingestion service before `create_or_get`.
/// - `metadata` JSON part with inline `payload` bytes, or with a
///   `payload_locator` naming a retrievable durable location.
async fn parse_operation_parts(
    multipart: &mut Multipart,
    max_upload_bytes: usize,
) -> Result<IngestionSubmission, Response> {
    let mut submission: Option<IngestionSubmission> = None;
    let mut payload: Option<Vec<u8>> = None;
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| bad_request(format!("invalid multipart: {e}")))?
    {
        match field.name() {
            Some("metadata") => {
                if submission.is_some() {
                    return Err(bad_request("duplicate metadata part"));
                }
                let bytes = field
                    .bytes()
                    .await
                    .map_err(|e| bad_request(format!("read metadata: {e}")))?;
                let sub = serde_json::from_slice(&bytes)
                    .map_err(|e| bad_request(format!("invalid metadata JSON: {e}")))?;
                submission = Some(sub);
            }
            Some("payload") => {
                if payload.is_some() {
                    return Err(bad_request("duplicate payload part"));
                }
                let bytes = field
                    .bytes()
                    .await
                    .map_err(|e| bad_request(format!("read payload: {e}")))?;
                if bytes.len() > max_upload_bytes {
                    return Err((StatusCode::PAYLOAD_TOO_LARGE,
                        Json(serde_json::json!({ "error": "payload exceeds the configured maximum upload size" }))).into_response());
                }
                payload = Some(bytes.to_vec());
            }
            _ => {}
        }
    }
    let mut submission = submission.ok_or_else(|| bad_request("missing metadata part"))?;
    if let Some(bytes) = payload {
        if submission.payload_locator.is_some() {
            return Err(bad_request(
                "payload part and metadata payload_locator are mutually exclusive",
            ));
        }
        submission.payload = Some(bytes);
    } else if submission.payload.is_none() && submission.payload_locator.is_none() {
        return Err(bad_request(
            "submission requires inline payload or a payload_locator",
        ));
    }
    if submission.payload.is_some() && submission.payload_locator.is_some() {
        return Err(bad_request(
            "payload and payload_locator are mutually exclusive",
        ));
    }
    Ok(submission)
}

/// POST /api/v1/ingestion-operations — durable, idempotent submission.
/// 202 for a new operation; 200 for an idempotent replay; 409 on conflicting
/// reuse of an idempotency key.
#[tracing::instrument(skip_all)]
pub async fn submit_operation(
    headers: HeaderMap,
    State(engine): State<Option<Arc<ArcanumEngine>>>,
    mut multipart: Multipart,
) -> impl IntoResponse {
    let start = std::time::Instant::now();
    let response: Response =
        {
            let claims = match validate_bearer(&headers, &engine) {
                Ok(c) => c,
                Err(e) => return e.into_response(),
            };
            let eng = engine.as_ref().unwrap();
            let submission =
                match parse_operation_parts(&mut multipart, eng.config.ingestion.max_upload_bytes)
                    .await
                {
                    Ok(s) => s,
                    Err(r) => return r,
                };
            if !eng
                .auth
                .can_access_collection(&claims, &submission.collection_id.0)
            {
                return (
                    StatusCode::FORBIDDEN,
                    Json(serde_json::json!({ "error": "access denied" })),
                )
                    .into_response();
            }
            match eng
                .ingestion
                .submit_operation(submission, false, &claims.user_id)
                .await
            {
                Ok((op_id, is_new)) => {
                    let status = if is_new {
                        StatusCode::ACCEPTED
                    } else {
                        StatusCode::OK
                    };
                    (
                        status,
                        Json(serde_json::json!({
                            "operation_id": op_id.0.to_string(),
                            "status": "accepted",
                            "resource": format!("/api/v1/ingestion-operations/{}", op_id.0),
                        })),
                    )
                        .into_response()
                }
                Err(e) => operation_error_response(&e),
            }
        };
    let elapsed = start.elapsed().as_secs_f64();
    let status = if response.status() == StatusCode::ACCEPTED || response.status() == StatusCode::OK
    {
        "ok"
    } else {
        "error"
    };
    counter!("arcanum_requests_total", "endpoint" => "submit_operation", "status" => status)
        .increment(1);
    histogram!("arcanum_request_duration_seconds", "endpoint" => "submit_operation")
        .record(elapsed);
    response
}

/// GET /api/v1/ingestion-operations/{id} — canonical durable operation document.
#[tracing::instrument(skip_all)]
pub async fn get_operation(
    headers: HeaderMap,
    State(engine): State<Option<Arc<ArcanumEngine>>>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    let start = std::time::Instant::now();
    let response: Response = {
        let claims = match validate_bearer(&headers, &engine) {
            Ok(c) => c,
            Err(e) => return e.into_response(),
        };
        let eng = engine.as_ref().unwrap();
        let operation = match eng.ingestion.operations().get(&OperationId(id)).await {
            Ok(Some(op)) => op,
            Ok(None) => {
                return operation_error_response(&ArcanumError::NotFound("operation".into()))
            }
            Err(e) => return operation_error_response(&e),
        };
        if !eng
            .auth
            .can_access_collection(&claims, &operation.submission.collection_id.0)
        {
            return (
                StatusCode::FORBIDDEN,
                Json(serde_json::json!({ "error": "access denied" })),
            )
                .into_response();
        }
        (StatusCode::OK, Json(operation)).into_response()
    };
    let elapsed = start.elapsed().as_secs_f64();
    let status = if response.status() == StatusCode::OK {
        "ok"
    } else {
        "error"
    };
    counter!("arcanum_requests_total", "endpoint" => "get_operation", "status" => status)
        .increment(1);
    histogram!("arcanum_request_duration_seconds", "endpoint" => "get_operation").record(elapsed);
    response
}

/// GET /api/v1/ingestion-operations?idempotency_key=KEY — requires EXACTLY one
/// query value; collection access is enforced against the resolved operation.
#[tracing::instrument(skip_all)]
pub async fn list_operation_by_idempotency(
    headers: HeaderMap,
    State(engine): State<Option<Arc<ArcanumEngine>>>,
    UrlQuery(params): UrlQuery<IdempotencyQuery>,
) -> impl IntoResponse {
    let start = std::time::Instant::now();
    let response: Response = {
        let claims = match validate_bearer(&headers, &engine) {
            Ok(c) => c,
            Err(e) => return e.into_response(),
        };
        let eng = engine.as_ref().unwrap();
        let operation = match eng
            .ingestion
            .operations()
            .get_by_idempotency(&params.idempotency_key)
            .await
        {
            Ok(Some(op)) => op,
            Ok(None) => {
                return operation_error_response(&ArcanumError::NotFound("operation".into()))
            }
            Err(e) => return operation_error_response(&e),
        };
        if !eng
            .auth
            .can_access_collection(&claims, &operation.submission.collection_id.0)
        {
            return (
                StatusCode::FORBIDDEN,
                Json(serde_json::json!({ "error": "access denied" })),
            )
                .into_response();
        }
        (StatusCode::OK, Json(operation)).into_response()
    };
    let elapsed = start.elapsed().as_secs_f64();
    let status = if response.status() == StatusCode::OK {
        "ok"
    } else {
        "error"
    };
    counter!("arcanum_requests_total", "endpoint" => "list_operation_by_idempotency", "status" => status).increment(1);
    histogram!("arcanum_request_duration_seconds", "endpoint" => "list_operation_by_idempotency")
        .record(elapsed);
    response
}

/// DELETE /api/v1/collections/{collectionId}/sources?source_uri=<encoded> —
/// idempotent removal of every entry for the stable source URI within one
/// collection. Uses the `delete_by_source_uri` contracts on the vector, graph,
/// tree, lexical (BM25) and chunk-registry stores and marks the
/// collection-scoped document versions deleted per the version-store contract.
/// Repeating removal for an absent source is a no-op success.
#[tracing::instrument(skip_all)]
pub async fn delete_collection_source(
    headers: HeaderMap,
    State(engine): State<Option<Arc<ArcanumEngine>>>,
    Path(collection_id): Path<String>,
    UrlQuery(params): UrlQuery<DeleteSourceParams>,
) -> impl IntoResponse {
    let start = std::time::Instant::now();
    let response: Response = {
        let claims = match validate_bearer(&headers, &engine) {
            Ok(c) => c,
            Err(e) => return e.into_response(),
        };
        let eng = engine.as_ref().unwrap();
        if !eng.auth.can_access_collection(&claims, &collection_id) {
            return (
                StatusCode::FORBIDDEN,
                Json(serde_json::json!({ "error": "access denied" })),
            )
                .into_response();
        }
        if let Some(store) = eng.vector_store.as_ref() {
            if let Err(e) = store
                .delete_by_source_uri(&collection_id, &params.source_uri)
                .await
            {
                tracing::warn!(collection = %collection_id, err = %e, "vector source removal failed");
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "error": "internal error" })),
                )
                    .into_response();
            }
        }
        if let Some(store) = eng.graph_store.as_ref() {
            if let Err(e) = store
                .delete_by_source_uri(&collection_id, &params.source_uri)
                .await
            {
                tracing::warn!(collection = %collection_id, err = %e, "graph source removal failed");
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "error": "internal error" })),
                )
                    .into_response();
            }
        }
        if let Some(store) = eng.tree_store.as_ref() {
            if let Err(e) = store
                .delete_by_source_uri(&collection_id, &params.source_uri)
                .await
            {
                tracing::warn!(collection = %collection_id, err = %e, "tree source removal failed");
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "error": "internal error" })),
                )
                    .into_response();
            }
        }
        if let Some(bm25) = eng.bm25_index.as_ref() {
            if let Err(e) = bm25.delete_by_source_uri(&collection_id, &params.source_uri) {
                tracing::warn!(collection = %collection_id, err = %e, "lexical source removal failed");
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "error": "internal error" })),
                )
                    .into_response();
            }
        }
        if let Some(store) = eng.chunk_metadata_store.as_ref() {
            if let Err(e) = store
                .delete_by_source_uri(&collection_id, &params.source_uri)
                .await
            {
                tracing::warn!(collection = %collection_id, err = %e, "chunk registry source removal failed");
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "error": "internal error" })),
                )
                    .into_response();
            }
        }
        if let Err(e) = eng
            .version_store
            .delete_by_source_uri(&collection_id, &params.source_uri)
            .await
        {
            tracing::warn!(collection = %collection_id, err = %e, "version source removal failed");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "internal error" })),
            )
                .into_response();
        }
        StatusCode::NO_CONTENT.into_response()
    };
    let elapsed = start.elapsed().as_secs_f64();
    let status = if response.status() == StatusCode::NO_CONTENT {
        "ok"
    } else {
        "error"
    };
    counter!("arcanum_requests_total", "endpoint" => "delete_collection_source", "status" => status).increment(1);
    histogram!("arcanum_request_duration_seconds", "endpoint" => "delete_collection_source")
        .record(elapsed);
    response
}

/// POST /api/v1/chunk/inspect — compare multiple chunking strategies on a text blob.
#[tracing::instrument(skip_all)]
pub async fn chunk_inspect(
    headers: HeaderMap,
    State(engine): State<Option<Arc<ArcanumEngine>>>,
    Json(req): Json<InspectRequest>,
) -> impl IntoResponse {
    let start = std::time::Instant::now();
    let response: Response = {
        let _claims = match validate_bearer(&headers, &engine) {
            Ok(c) => c,
            Err(e) => return e.into_response(),
        };
        match inspect(&req.text, &req.strategies).await {
            Ok(results) => (
                StatusCode::OK,
                axum::Json(serde_json::json!({ "results": results })),
            )
                .into_response(),
            Err(e) => (
                StatusCode::BAD_REQUEST,
                axum::Json(serde_json::json!({ "error": e.to_string() })),
            )
                .into_response(),
        }
    };
    let elapsed = start.elapsed().as_secs_f64();
    let status = if response.status() == StatusCode::OK {
        "ok"
    } else {
        "error"
    };
    counter!("arcanum_requests_total", "endpoint" => "chunk_inspect", "status" => status)
        .increment(1);
    histogram!("arcanum_request_duration_seconds", "endpoint" => "chunk_inspect").record(elapsed);
    response
}

/// POST /api/v1/chunk/benchmark — run offline benchmark on a corpus.
#[tracing::instrument(skip_all)]
pub async fn chunk_benchmark(
    headers: HeaderMap,
    State(engine): State<Option<Arc<ArcanumEngine>>>,
    Json(req): Json<BenchmarkJob>,
) -> impl IntoResponse {
    let start = std::time::Instant::now();
    let response: Response = {
        let _claims = match validate_bearer(&headers, &engine) {
            Ok(c) => c,
            Err(e) => return e.into_response(),
        };
        // Benchmark is synchronous for typical test corpora.
        match run_benchmark(req).await {
            Ok(metrics) => (
                StatusCode::OK,
                axum::Json(serde_json::json!({ "metrics": metrics })),
            )
                .into_response(),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                axum::Json(serde_json::json!({ "error": e.to_string() })),
            )
                .into_response(),
        }
    };
    let elapsed = start.elapsed().as_secs_f64();
    let status = if response.status() == StatusCode::OK {
        "ok"
    } else {
        "error"
    };
    counter!("arcanum_requests_total", "endpoint" => "chunk_benchmark", "status" => status)
        .increment(1);
    histogram!("arcanum_request_duration_seconds", "endpoint" => "chunk_benchmark").record(elapsed);
    response
}

#[cfg(test)]
mod upload_tests {
    use crate::build_app;
    use axum::body::Body;
    use axum::http::{Method, Request, StatusCode};
    use tower::ServiceExt;

    #[tokio::test]
    async fn upload_requires_auth() {
        let app = build_app(None);
        let resp = app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/upload?collection_id=c&filename=f.md")
                    .body(Body::from("hello"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn upload_force_param_is_accepted() {
        use arcanum_core::traits::NoOpDocumentVersionStore;
        use arcanum_engine::ArcanumEngine;
        use std::sync::Arc;
        let engine = ArcanumEngine::builder()
            .auth_secret("a-32-char-secret-for-testing-ok!")
            .version_store(Arc::new(NoOpDocumentVersionStore))
            .build()
            .await
            .unwrap();
        let token = engine.auth.generate_admin_key("tester");
        let app = crate::build_app(Some(engine));
        let resp = app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/upload?collection_id=c&filename=f.md&force=true")
                    .header("Authorization", format!("Bearer {token}"))
                    .body(Body::from(b"# Hello\nworld".to_vec()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::ACCEPTED);
    }
}

#[cfg(test)]
mod chunk_route_tests {
    use crate::build_app;
    use axum::body::Body;
    use axum::http::{Method, Request, StatusCode};
    use tower::ServiceExt;

    #[tokio::test]
    async fn chunk_inspect_requires_auth() {
        let app = build_app(None);
        let resp = app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/chunk/inspect")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"text":"hello world","strategies":[]}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }
}

#[cfg(test)]
mod rate_limit_tests {
    use crate::build_app;
    use axum::body::Body;
    use axum::http::{Method, Request, StatusCode};
    use tower::ServiceExt;

    #[tokio::test]
    async fn requests_beyond_the_window_return_429() {
        use arcanum_core::traits::NoOpDocumentVersionStore;
        use arcanum_engine::ArcanumEngine;
        use std::sync::Arc;
        let engine = ArcanumEngine::builder()
            .auth_secret("a-32-char-secret-for-testing-ok!")
            .version_store(Arc::new(NoOpDocumentVersionStore))
            .build()
            .await
            .unwrap();
        let token = engine.auth.generate_admin_key("tester");
        let claims = engine.auth.validate_api_key(&token).unwrap();

        // Drain this caller's window directly, without N real HTTP round trips.
        while engine.rate_limiter.check_and_record(&claims.user_id) {}

        let app = build_app(Some(engine));
        let resp = app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/chunk/inspect")
                    .header("Authorization", format!("Bearer {token}"))
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"text":"hello","strategies":[]}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);
    }
}

#[cfg(test)]
mod context_status_tests {
    use super::*;
    use arcanum_engine::services::context::ContextError;

    #[test]
    fn context_error_status_maps_each_variant() {
        assert_eq!(
            context_error_status(&ContextError::Invalid("x".into())),
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            context_error_status(&ContextError::Forbidden("x".into())),
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            context_error_status(&ContextError::Unavailable("x".into())),
            StatusCode::SERVICE_UNAVAILABLE
        );
        assert_eq!(
            context_error_status(&ContextError::Internal(ArcanumError::Storage("x".into()))),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }
}
