use axum::{extract::{State, Json}, http::{StatusCode, HeaderMap}, response::IntoResponse};
use serde::Deserialize;
use std::sync::Arc;
use arcanum_core::types::{Query, CollectionId};
use arcanum_engine::{ArcanumEngine, auth::ApiKeyClaims};

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
}

pub async fn search(
    headers: HeaderMap,
    State(engine): State<Option<Arc<ArcanumEngine>>>,
    Json(req): Json<SearchRequest>,
) -> impl IntoResponse {
    let claims = match validate_bearer(&headers, &engine) {
        Ok(c) => c,
        Err(e) => return e.into_response(),
    };
    let eng = engine.as_ref().unwrap();
    // Verify collection access.
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
}

pub async fn ingest(
    headers: HeaderMap,
    State(engine): State<Option<Arc<ArcanumEngine>>>,
    Json(req): Json<HttpIngestRequest>,
) -> impl IntoResponse {
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
        force: false,
    };

    match eng.ingestion.ingest(ingest_req, &claims.user_id).await {
        Ok(op_id) => (StatusCode::ACCEPTED, axum::Json(serde_json::json!({
            "operation_id": op_id.0,
            "status": "queued"
        }))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR,
            axum::Json(serde_json::json!({ "error": e.to_string() }))).into_response(),
    }
}
