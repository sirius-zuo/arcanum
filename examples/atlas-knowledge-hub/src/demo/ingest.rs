use super::{require_key, DemoCtx, DemoError};
use crate::engine_setup::COLLECTION;
use crate::samples::{read_sample, SampleFile};
use arcanum_core::types::{CollectionId, IngestionSubmission, OperationId, OperationStatus};
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::path::Path;
use std::time::{Duration, Instant};

#[derive(Serialize)]
pub struct OperationRef {
    pub source_uri: String,
    pub operation_id: String,
}

#[derive(Serialize)]
pub struct IngestResponse {
    pub operations: Vec<OperationRef>,
}

/// Submits each file and returns 202 when at least one submission is new, 200 when every
/// submission was an idempotent replay.
async fn submit_files(
    ctx: &DemoCtx,
    files: Vec<&SampleFile>,
) -> Result<(StatusCode, Json<IngestResponse>), DemoError> {
    // Read per request: the env var is process-global and tests change it.
    let template = std::env::var("ATLAS_PIPELINE").unwrap_or_else(|_| "full".to_string());
    let mut operations = Vec::new();
    let mut any_new = false;
    for f in files {
        let bytes = read_sample(Path::new("samples"), f)
            .map_err(|e| DemoError::Internal(format!("{e:#}")))?;
        let mut hasher = Sha256::new();
        hasher.update(format!("{COLLECTION}:{}:", f.source_uri).as_bytes());
        hasher.update(&bytes);
        let submission = IngestionSubmission {
            idempotency_key: hex(&hasher.finalize()),
            logical_source_uri: f.source_uri.clone(),
            mime_hint: Some("text/markdown".into()),
            collection_id: CollectionId(COLLECTION.into()),
            pipeline_configuration: serde_json::json!({ "template": template }),
            payload: Some(bytes),
            payload_locator: None,
        };
        let (id, is_new) = ctx
            .state
            .engine
            .ingestion
            .submit_operation(submission, false, "atlas")
            .await
            .map_err(|e| DemoError::Internal(format!("ingest {}: {e}", f.source_uri)))?;
        if is_new && !any_new {
            // The vector store creates its table on first upsert without guarding against a
            // concurrent creator, so let the first new operation finish before the rest start.
            wait_terminal(ctx, &id).await;
        }
        any_new |= is_new;
        operations.push(OperationRef {
            source_uri: f.source_uri.clone(),
            operation_id: id.0.to_string(),
        });
    }
    let status = if any_new {
        StatusCode::ACCEPTED
    } else {
        StatusCode::OK
    };
    Ok((status, Json(IngestResponse { operations })))
}

/// Polls until the operation is terminal, giving up after two minutes (the operation keeps
/// running either way).
async fn wait_terminal(ctx: &DemoCtx, id: &OperationId) {
    let ops = ctx.state.engine.ingestion.operations();
    let deadline = Instant::now() + Duration::from_secs(120);
    while Instant::now() < deadline {
        match ops.get(id).await {
            Ok(Some(op))
                if !matches!(
                    op.status,
                    OperationStatus::Succeeded | OperationStatus::Failed
                ) => {}
            _ => return,
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub async fn load(
    State(ctx): State<DemoCtx>,
    headers: HeaderMap,
) -> Result<(StatusCode, Json<IngestResponse>), DemoError> {
    require_key(&ctx.state, &headers)?;
    let manifest = ctx.manifest.clone();
    submit_files(
        &ctx,
        manifest.files.iter().filter(|f| !f.is_update).collect(),
    )
    .await
}

pub async fn apply_update(
    State(ctx): State<DemoCtx>,
    headers: HeaderMap,
) -> Result<(StatusCode, Json<IngestResponse>), DemoError> {
    require_key(&ctx.state, &headers)?;
    let manifest = ctx.manifest.clone();
    submit_files(
        &ctx,
        manifest.files.iter().filter(|f| f.is_update).collect(),
    )
    .await
}
