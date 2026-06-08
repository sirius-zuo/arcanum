use arcanum_core::{types::{CollectionId, PerBackendChunkConfig, ExperimentId}};
use arcanum_engine::ArcanumEngine;
use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    Json,
};
use std::sync::Arc;
use crate::routes::auth::validate_bearer;

/// POST /api/v1/collections/{collection_id}/experiments
pub async fn start_experiment(
    headers: HeaderMap,
    State(engine): State<Option<Arc<ArcanumEngine>>>,
    Path(collection_id): Path<String>,
    Json(challenger_config): Json<PerBackendChunkConfig>,
) -> impl IntoResponse {
    let _claims = match validate_bearer(&headers, &engine) {
        Ok(c) => c,
        Err(e) => return e.into_response(),
    };
    let eng = engine.as_ref().unwrap();
    match eng.experiment.start(CollectionId(collection_id), challenger_config).await {
        Ok(exp) => (StatusCode::CREATED, Json(serde_json::json!({
            "experiment_id": exp.id.0.to_string(),
            "status": exp.status,
            "started_at": exp.started_at,
            "challenger_config": exp.challenger_config,
        }))).into_response(),
        Err(e) if e.to_string().contains("active") => {
            (StatusCode::CONFLICT, Json(serde_json::json!({ "error": e.to_string() }))).into_response()
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() }))).into_response(),
    }
}

/// GET /api/v1/collections/{collection_id}/experiments/{experiment_id}
pub async fn get_experiment(
    headers: HeaderMap,
    State(engine): State<Option<Arc<ArcanumEngine>>>,
    Path((collection_id, experiment_id)): Path<(String, uuid::Uuid)>,
) -> impl IntoResponse {
    let _claims = match validate_bearer(&headers, &engine) {
        Ok(c) => c,
        Err(e) => return e.into_response(),
    };
    let eng = engine.as_ref().unwrap();
    let exp_id = ExperimentId(experiment_id);
    match eng.experiment.get(&collection_id, &exp_id).await {
        Ok(exp) => (StatusCode::OK, Json(serde_json::json!({
            "experiment_id": exp.id.0.to_string(),
            "status": exp.status,
            "challenger_config": exp.challenger_config,
            "metrics": exp.metrics,
            "started_at": exp.started_at,
        }))).into_response(),
        Err(e) => (StatusCode::NOT_FOUND, Json(serde_json::json!({ "error": e.to_string() }))).into_response(),
    }
}

/// POST /api/v1/collections/{collection_id}/experiments/{experiment_id}/promote
pub async fn promote_experiment(
    headers: HeaderMap,
    State(engine): State<Option<Arc<ArcanumEngine>>>,
    Path((collection_id, experiment_id)): Path<(String, uuid::Uuid)>,
) -> impl IntoResponse {
    let _claims = match validate_bearer(&headers, &engine) {
        Ok(c) => c,
        Err(e) => return e.into_response(),
    };
    let eng = engine.as_ref().unwrap();
    let exp_id = ExperimentId(experiment_id);
    match eng.experiment.promote(&collection_id, &exp_id).await {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({
            "status": "closed",
            "message": "challenger config promoted; new documents will use the promoted strategy"
        }))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() }))).into_response(),
    }
}

/// DELETE /api/v1/collections/{collection_id}/experiments/{experiment_id}
pub async fn abandon_experiment(
    headers: HeaderMap,
    State(engine): State<Option<Arc<ArcanumEngine>>>,
    Path((collection_id, experiment_id)): Path<(String, uuid::Uuid)>,
) -> impl IntoResponse {
    let _claims = match validate_bearer(&headers, &engine) {
        Ok(c) => c,
        Err(e) => return e.into_response(),
    };
    let eng = engine.as_ref().unwrap();
    let exp_id = ExperimentId(experiment_id);
    match eng.experiment.abandon(&collection_id, &exp_id).await {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({ "status": "closed" }))).into_response(),
        Err(e) => (StatusCode::NOT_FOUND, Json(serde_json::json!({ "error": e.to_string() }))).into_response(),
    }
}
