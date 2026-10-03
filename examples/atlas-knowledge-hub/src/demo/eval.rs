use super::{require_key, DemoCtx, DemoError};
use crate::engine_setup::COLLECTION;
use arcanum_core::types::{ChunkBackend, ChunkId, CollectionId, Query};
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

/// 1-based rank of the first ranked chunk that is relevant.
pub fn first_relevant_rank(ranked: &[ChunkId], relevant: &HashSet<ChunkId>) -> Option<usize> {
    ranked
        .iter()
        .position(|id| relevant.contains(id))
        .map(|i| i + 1)
}

/// Vector-backend chunk ids of the latest version of each source uri in `halcyon`.
async fn latest_chunks_by_source(ctx: &DemoCtx) -> HashMap<String, HashSet<ChunkId>> {
    let mut latest: HashMap<String, u32> = HashMap::new();
    let records: Vec<_> = ctx
        .state
        .registry
        .get_all()
        .await
        .into_iter()
        .filter(|r| r.collection_id == COLLECTION && r.backend == ChunkBackend::Vector)
        .collect();
    for r in &records {
        let v = latest.entry(r.source_uri.clone()).or_default();
        *v = (*v).max(r.version_num);
    }
    let mut out: HashMap<String, HashSet<ChunkId>> = HashMap::new();
    for r in records {
        if latest.get(&r.source_uri) == Some(&r.version_num) {
            out.entry(r.source_uri).or_default().insert(r.chunk_id);
        }
    }
    out
}

/// `POST /demo/eval`: runs the golden set against live search and reports retrieval metrics.
pub async fn eval(
    State(ctx): State<DemoCtx>,
    headers: HeaderMap,
) -> Result<Json<EvalView>, DemoError> {
    require_key(&ctx.state, &headers)?;
    let by_source = latest_chunks_by_source(&ctx).await;

    let mut samples = Vec::new();
    for g in &ctx.manifest.golden {
        let relevant = by_source
            .get(&g.relevant_source_uri)
            .filter(|s| !s.is_empty())
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
        let ranked: Vec<ChunkId> = result
            .chunks
            .iter()
            .map(|c| c.indexed_chunk.chunk.id.clone())
            .collect();
        queries.push(QueryView {
            query: g.query.clone(),
            relevant_source_uri: g.relevant_source_uri.clone(),
            first_relevant_rank: first_relevant_rank(&ranked, relevant),
        });
        golden_samples.push(GoldenSample {
            query: g.query.clone(),
            relevant_chunk_ids: relevant.iter().cloned().collect(),
        });
        ranked_lists.push(ranked);
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
