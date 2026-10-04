use super::{require_key, DemoCtx, DemoError};
use crate::engine_setup::COLLECTION;
use arcanum_core::types::{ChunkBackend, DocumentVersion};
use axum::extract::State;
use axum::http::HeaderMap;
use axum::Json;
use serde::Serialize;
use std::collections::HashMap;

#[derive(Serialize)]
pub struct VersionView {
    pub version_num: u32,
    pub status: String,
    pub ingested_at: String,
    pub content_hash: String,
    pub snapshot_uri: String,
}

#[derive(Serialize)]
pub struct DocumentView {
    pub source_uri: String,
    pub document_id: String,
    pub chunks: usize,
    pub versions: Vec<VersionView>,
}

#[derive(Serialize)]
pub struct LibraryView {
    pub collection: String,
    pub documents: Vec<DocumentView>,
}

fn version_view(v: &DocumentVersion) -> VersionView {
    VersionView {
        version_num: v.version_num,
        status: format!("{:?}", v.status),
        ingested_at: v.ingested_at.to_rfc3339(),
        content_hash: v.content_hash.clone(),
        snapshot_uri: v.snapshot_uri.clone(),
    }
}

fn internal<E: std::fmt::Display>(e: E) -> DemoError {
    DemoError::Internal(e.to_string())
}

/// `GET /demo/library`: every document with its version history and live chunk count.
pub async fn library(
    State(ctx): State<DemoCtx>,
    headers: HeaderMap,
) -> Result<Json<LibraryView>, DemoError> {
    require_key(&ctx.state, &headers)?;
    let store = &ctx.state.engine.version_store;

    // Vector-backend registry records per (source_uri, version_num).
    let mut counts: HashMap<(String, u32), usize> = HashMap::new();
    for r in ctx.state.registry.get_all().await {
        if r.collection_id == COLLECTION && r.backend == ChunkBackend::Vector {
            *counts.entry((r.source_uri, r.version_num)).or_default() += 1;
        }
    }

    let mut entries = store.list_documents(COLLECTION).await.map_err(internal)?;
    entries.sort_by(|a, b| a.source_uri.cmp(&b.source_uri));
    let mut documents = Vec::new();
    for e in entries {
        let Some(latest) = store
            .get_latest(&e.source_uri, COLLECTION)
            .await
            .map_err(internal)?
        else {
            continue;
        };
        let mut versions = store
            .list_versions(&latest.document_id)
            .await
            .map_err(internal)?;
        versions.sort_by_key(|v| v.version_num);
        let chunks = counts
            .get(&(e.source_uri.clone(), latest.version_num))
            .copied()
            .unwrap_or(0);
        documents.push(DocumentView {
            source_uri: e.source_uri,
            document_id: latest.document_id.0.to_string(),
            chunks,
            versions: versions.iter().map(version_view).collect(),
        });
    }
    Ok(Json(LibraryView {
        collection: COLLECTION.into(),
        documents,
    }))
}
