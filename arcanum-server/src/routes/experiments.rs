use arcanum_core::{types::{ChunkId, CollectionId, PerBackendChunkConfig, ExperimentId, Query}};
use arcanum_engine::{ArcanumEngine, auth::ApiKeyClaims, services::experiment::ExperimentMetrics};
use arcanum_eval::{EvalRunner, GoldenSample};
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

/// POST /api/v1/collections/{collection_id}/experiments/{experiment_id}/eval
///
/// Caller supplies labeled queries (no persistent benchmark-query store
/// exists yet — see the background eval loop's own stub comment in
/// engine.rs). Runs each query against both the collection's live
/// namespace (champion) and the experiment's shadow namespace
/// (challenger), computes recall@5 for each side via EvalRunner, and
/// feeds the result into ExperimentService::update_metrics — the only
/// path that can flip an experiment to ReadyToPromote.
pub async fn eval_experiment(
    headers: HeaderMap,
    State(engine): State<Option<Arc<ArcanumEngine>>>,
    Path((collection_id, experiment_id)): Path<(String, uuid::Uuid)>,
    Json(samples): Json<Vec<GoldenSample>>,
) -> impl IntoResponse {
    let claims = match validate_bearer(&headers, &engine) {
        Ok(c) => c,
        Err(e) => return e.into_response(),
    };
    let eng = engine.as_ref().unwrap();
    if !eng.auth.can_access_collection(&claims, &collection_id) {
        return (StatusCode::FORBIDDEN,
            Json(serde_json::json!({ "error": "access denied" }))).into_response();
    }
    let exp_id = ExperimentId(experiment_id);
    let exp = match eng.experiment.get(&collection_id, &exp_id).await {
        Ok(e) => e,
        Err(e) => return (StatusCode::NOT_FOUND, Json(serde_json::json!({ "error": e.to_string() }))).into_response(),
    };
    let shadow_namespace = exp.shadow_namespace(&collection_id);

    // Internal system claims for the two namespace searches below — not
    // derived from the caller's token. The shadow namespace is a synthetic
    // name the caller's own allowed_collections ACL was never meant to
    // describe; the access check above already confirmed the caller may
    // manage this experiment on the parent collection.
    let system_claims = ApiKeyClaims {
        user_id: "system:experiment-eval".into(),
        allowed_collections: vec![],
        is_admin: true,
        exp: claims.exp,
    };

    let mut champion_results: Vec<Vec<ChunkId>> = Vec::with_capacity(samples.len());
    let mut challenger_results: Vec<Vec<ChunkId>> = Vec::with_capacity(samples.len());
    for sample in &samples {
        let champion_query = Query::new(sample.query.clone())
            .with_collection(CollectionId(collection_id.clone()))
            .with_top_k(5);
        let challenger_query = Query::new(sample.query.clone())
            .with_collection(CollectionId(shadow_namespace.clone()))
            .with_top_k(5);
        let (champion, challenger) = match (
            eng.retrieval.search(champion_query, &system_claims).await,
            eng.retrieval.search(challenger_query, &system_claims).await,
        ) {
            (Ok(c), Ok(h)) => (c, h),
            (Err(e), _) | (_, Err(e)) => {
                return (StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "error": e.to_string() }))).into_response();
            }
        };
        champion_results.push(champion.chunks.iter().map(|c| c.indexed_chunk.chunk.id.clone()).collect());
        challenger_results.push(challenger.chunks.iter().map(|c| c.indexed_chunk.chunk.id.clone()).collect());
    }

    let runner = EvalRunner::new(5);
    let champion_report = runner.evaluate(&champion_results, &samples);
    let challenger_report = runner.evaluate(&challenger_results, &samples);

    let metrics = ExperimentMetrics {
        champion_recall_at_5:   champion_report.recall_at_k,
        challenger_recall_at_5: challenger_report.recall_at_k,
        sample_size:            samples.len(),
        computed_at:            chrono::Utc::now().to_rfc3339(),
    };

    match eng.experiment.update_metrics(&collection_id, &exp_id, metrics.clone()).await {
        Ok(status) => (StatusCode::OK, Json(serde_json::json!({
            "status": status,
            "metrics": metrics,
        }))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() }))).into_response(),
    }
}
