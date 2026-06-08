use arcanum_core::{Result, types::{ChunkStrategyConfig, DocumentId, RawDocument}};
use arcanum_ingestion::default_registry;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LabeledQuery {
    pub text:             String,
    pub expected_doc_ids: Vec<DocumentId>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkJob {
    pub corpus:     Vec<RawDocument>,
    pub queries:    Vec<LabeledQuery>,
    pub strategies: Vec<ChunkStrategyConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkMetrics {
    pub strategy:          ChunkStrategyConfig,
    pub recall_at_5:       f32,
    pub recall_at_10:      f32,
    pub mean_chunk_tokens: f32,
    pub chunk_size_p50:    f32,
    pub chunk_size_p95:    f32,
}

/// Retrieves the top-k chunks for a query using token-overlap scoring.
/// Returns the document_ids of retrieved chunks in ranked order.
/// No embedding model required — deterministic text overlap.
fn retrieve_by_overlap(
    query: &str,
    chunks: &[(DocumentId, String)],  // (doc_id, chunk_text)
    top_k: usize,
) -> Vec<DocumentId> {
    let query_tokens: HashSet<&str> = query.split_whitespace().collect();

    let mut scored: Vec<(f32, &DocumentId)> = chunks.iter()
        .map(|(doc_id, text)| {
            let chunk_tokens: HashSet<&str> = text.split_whitespace().collect();
            let overlap = query_tokens.intersection(&chunk_tokens).count() as f32;
            (overlap, doc_id)
        })
        .filter(|(score, _)| *score > 0.0)
        .collect();

    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    scored.iter().take(top_k).map(|(_, id)| (*id).clone()).collect()
}

fn recall_at_k(retrieved: &[DocumentId], expected: &[DocumentId], k: usize) -> f32 {
    if expected.is_empty() { return 1.0; }
    let top_k_ids: HashSet<String> = retrieved.iter()
        .take(k)
        .map(|id| id.0.to_string())
        .collect();
    let expected_ids: HashSet<String> = expected.iter()
        .map(|id| id.0.to_string())
        .collect();
    let hits = top_k_ids.intersection(&expected_ids).count();
    hits as f32 / expected.len() as f32
}

fn percentile(sorted: &[f32], p: f32) -> f32 {
    if sorted.is_empty() { return 0.0; }
    let idx = ((p / 100.0) * (sorted.len() - 1) as f32).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

pub async fn run_benchmark(job: BenchmarkJob) -> Result<Vec<BenchmarkMetrics>> {
    let registry = default_registry();
    let mut results = Vec::with_capacity(job.strategies.len());

    for strategy in &job.strategies {
        let chunker = registry.build(strategy)?;
        let mut all_chunks: Vec<(DocumentId, String)> = Vec::new();
        let mut token_counts: Vec<f32> = Vec::new();

        for doc in &job.corpus {
            let chunks = chunker.chunk(doc).await?;
            for c in &chunks {
                let token_count = (c.text.chars().count() / 4).max(1) as f32;
                token_counts.push(token_count);
                all_chunks.push((doc.id.clone(), c.text.clone()));
            }
        }

        // Compute chunk size distribution
        token_counts.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let mean_chunk_tokens = if token_counts.is_empty() { 0.0 }
            else { token_counts.iter().sum::<f32>() / token_counts.len() as f32 };
        let chunk_size_p50 = percentile(&token_counts, 50.0);
        let chunk_size_p95 = percentile(&token_counts, 95.0);

        // Compute recall metrics
        let mut recall5_sum = 0.0f32;
        let mut recall10_sum = 0.0f32;

        for query in &job.queries {
            let retrieved_5  = retrieve_by_overlap(&query.text, &all_chunks, 5);
            let retrieved_10 = retrieve_by_overlap(&query.text, &all_chunks, 10);
            recall5_sum  += recall_at_k(&retrieved_5,  &query.expected_doc_ids, 5);
            recall10_sum += recall_at_k(&retrieved_10, &query.expected_doc_ids, 10);
        }

        let n = job.queries.len().max(1) as f32;
        results.push(BenchmarkMetrics {
            strategy: strategy.clone(),
            recall_at_5:       recall5_sum  / n,
            recall_at_10:      recall10_sum / n,
            mean_chunk_tokens,
            chunk_size_p50,
            chunk_size_p95,
        });
    }

    Ok(results)
}
