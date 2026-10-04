use super::{require_key, DemoCtx, DemoError};
use crate::engine_setup::COLLECTION;
use arcanum_core::types::{ChunkId, CollectionId, DocumentId, Query};
use arcanum_eval::{EvalRunner, GoldenSample};
use axum::extract::State;
use axum::http::HeaderMap;
use axum::Json;
use serde::Serialize;
use std::collections::{HashMap, HashSet};

const K: usize = 5;

#[derive(Serialize)]
pub struct ReportView {
    pub hit_rate_at_k: f32,
    pub mrr: f32,
    pub ndcg_at_k: f32,
    pub k: usize,
    pub num_queries: usize,
    pub precision_at_k: f32,
    pub recall_at_k: f32,
}

#[derive(Serialize)]
pub struct QueryView {
    pub query: String,
    pub relevant_source_uri: String,
    pub first_relevant_rank: Option<usize>,
}

#[derive(Serialize)]
pub struct EvalView {
    pub report: ReportView,
    pub queries: Vec<QueryView>,
}

/// 1-based rank of the first ranked item that is relevant.
pub fn first_relevant_rank<T: Eq + std::hash::Hash>(
    ranked: &[T],
    relevant: &HashSet<T>,
) -> Option<usize> {
    ranked
        .iter()
        .position(|id| relevant.contains(id))
        .map(|i| i + 1)
}

/// Document ids (any backend) of the latest version of each source uri in `halcyon`.
async fn latest_documents_by_source(ctx: &DemoCtx) -> HashMap<String, HashSet<DocumentId>> {
    let records: Vec<_> = ctx
        .state
        .registry
        .get_all()
        .await
        .into_iter()
        .filter(|r| r.collection_id == COLLECTION)
        .collect();
    let mut latest: HashMap<String, u32> = HashMap::new();
    for r in &records {
        let v = latest.entry(r.source_uri.clone()).or_default();
        *v = (*v).max(r.version_num);
    }
    let mut out: HashMap<String, HashSet<DocumentId>> = HashMap::new();
    for r in records {
        if latest.get(&r.source_uri) == Some(&r.version_num) {
            out.entry(r.source_uri).or_default().insert(r.document_id);
        }
    }
    out
}

/// EvalRunner is keyed by `ChunkId`; both ids are thin `Uuid` newtypes, so a document id maps
/// losslessly onto one.
fn as_chunk_id(d: &DocumentId) -> ChunkId {
    ChunkId(d.0)
}

/// `POST /demo/eval`: runs the golden set against live search and reports retrieval metrics.
/// Metrics are document-level: fusion returns one chunk per document, so chunk-level relevance
/// (all chunks of the source) would cap recall and precision near zero.
pub async fn eval(
    State(ctx): State<DemoCtx>,
    headers: HeaderMap,
) -> Result<Json<EvalView>, DemoError> {
    require_key(&ctx.state, &headers)?;
    if ctx.manifest.golden.is_empty() {
        return Err(DemoError::Conflict("no golden queries".into()));
    }
    let by_source = latest_documents_by_source(&ctx).await;

    let mut samples = Vec::new();
    for g in &ctx.manifest.golden {
        let relevant = by_source
            .get(&g.relevant_source_uri)
            .ok_or_else(|| DemoError::Conflict("corpus not loaded".into()))?;
        samples.push((g, relevant));
    }

    let mut ranked_lists = Vec::new();
    let mut golden_samples = Vec::new();
    let mut queries = Vec::new();
    for (g, relevant) in samples {
        let query = Query::new(&g.query)
            .with_collection(CollectionId(COLLECTION.into()))
            .with_top_k(K);
        let result = ctx
            .state
            .engine
            .retrieval
            .search(query, &ctx.state.claims)
            .await
            .map_err(|e| DemoError::Internal(e.to_string()))?;
        let mut ranked: Vec<DocumentId> = Vec::new();
        for c in &result.chunks {
            let d = &c.indexed_chunk.chunk.document_id;
            if !ranked.contains(d) {
                ranked.push(d.clone());
            }
        }
        queries.push(QueryView {
            query: g.query.clone(),
            relevant_source_uri: g.relevant_source_uri.clone(),
            first_relevant_rank: first_relevant_rank(&ranked, relevant),
        });
        golden_samples.push(GoldenSample {
            query: g.query.clone(),
            relevant_chunk_ids: relevant.iter().map(as_chunk_id).collect(),
        });
        ranked_lists.push(ranked.iter().map(as_chunk_id).collect());
    }

    let r = EvalRunner::new(K).evaluate(&ranked_lists, &golden_samples);
    Ok(Json(EvalView {
        report: ReportView {
            hit_rate_at_k: r.hit_rate_at_k,
            mrr: r.mrr,
            ndcg_at_k: r.ndcg_at_k,
            k: r.k,
            num_queries: r.num_queries,
            precision_at_k: r.precision_at_k,
            recall_at_k: r.recall_at_k,
        },
        queries,
    }))
}
