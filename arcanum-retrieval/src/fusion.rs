use arcanum_core::types::*;
use std::collections::HashMap;

pub struct RrfFusion;

impl RrfFusion {
    pub fn fuse(
        strategy_results: Vec<(RetrievalStrategy, Vec<RetrievedChunk>)>,
        k: f32,
    ) -> Vec<RetrievedChunk> {
        let mut scores: HashMap<String, (f32, RetrievedChunk)> = HashMap::new();
        for (_strategy, chunks) in strategy_results {
            for (rank, chunk) in chunks.into_iter().enumerate() {
                let rrf_score = 1.0 / (k + rank as f32 + 1.0);
                let key = chunk.indexed_chunk.chunk.text.clone();
                scores.entry(key)
                    .and_modify(|(s, _)| *s += rrf_score)
                    .or_insert((rrf_score, chunk));
            }
        }
        let mut result: Vec<(f32, RetrievedChunk)> = scores.into_values().collect();
        result.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        result.into_iter().map(|(score, mut c)| { c.score = score; c }).collect()
    }
}
