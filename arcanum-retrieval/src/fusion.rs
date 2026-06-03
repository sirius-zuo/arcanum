use arcanum_core::types::*;
use std::collections::HashMap;
use tracing::instrument;

pub struct RrfFusion;

impl RrfFusion {
    #[instrument(fields(strategy_count = strategy_results.len(), result_count))]
    pub fn fuse(
        strategy_results: Vec<(RetrievalStrategy, Vec<RetrievedChunk>)>,
        k: f32,
    ) -> Vec<RetrievedChunk> {
        let mut scores: HashMap<String, (f32, RetrievedChunk)> = HashMap::new();
        for (_strategy, chunks) in strategy_results {
            for (rank, chunk) in chunks.into_iter().enumerate() {
                let rrf_score = 1.0 / (k + rank as f32 + 1.0);
                // Key on stable ChunkId, not text — prevents wrong-record retention
                // when two chunks share identical text but differ in provenance.
                let key = chunk.indexed_chunk.chunk.id.0.to_string();
                scores.entry(key)
                    .and_modify(|(s, _)| *s += rrf_score)
                    .or_insert((rrf_score, chunk));
            }
        }
        let mut result: Vec<(f32, RetrievedChunk)> = scores.into_values().collect();
        result.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        let result: Vec<RetrievedChunk> = result.into_iter().map(|(score, mut c)| { c.score = score; c }).collect();
        tracing::Span::current().record("result_count", result.len());
        result
    }
}

// ---------------------------------------------------------------------------
// WeightedFusion
// ---------------------------------------------------------------------------

/// Fuses results from multiple strategies by multiplying each chunk's score
/// by the weight configured for its strategy. Chunks appearing in multiple
/// strategies are merged by taking the maximum weighted score.
///
/// `weights` is a slice of `(strategy_debug_name, weight)` pairs where the
/// strategy name matches `format!("{:?}", RetrievalStrategy::*)` e.g. "Vector".
pub struct WeightedFusion;

impl WeightedFusion {
    #[instrument(fields(strategy_count = strategy_results.len(), result_count))]
    pub fn fuse(
        strategy_results: Vec<(RetrievalStrategy, Vec<RetrievedChunk>)>,
        weights: &[(String, f32)],
    ) -> Vec<RetrievedChunk> {
        let weight_map: HashMap<&str, f32> = weights.iter()
            .map(|(name, w)| (name.as_str(), *w))
            .collect();

        // ChunkId → (best_weighted_score, chunk)
        let mut best: HashMap<String, (f32, RetrievedChunk)> = HashMap::new();

        for (strategy, chunks) in strategy_results {
            let strategy_name = format!("{:?}", strategy);
            let weight = weight_map.get(strategy_name.as_str()).copied().unwrap_or(1.0);

            for mut chunk in chunks {
                let weighted = chunk.score * weight;
                let key = chunk.indexed_chunk.chunk.id.0.to_string();
                chunk.score = weighted;
                best.entry(key)
                    .and_modify(|(best_score, _)| {
                        if weighted > *best_score { *best_score = weighted; }
                    })
                    .or_insert((weighted, chunk));
            }
        }

        let mut result: Vec<(f32, RetrievedChunk)> = best.into_values().collect();
        result.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        let result: Vec<RetrievedChunk> = result.into_iter().map(|(score, mut c)| { c.score = score; c }).collect();
        tracing::Span::current().record("result_count", result.len());
        result
    }
}

// ---------------------------------------------------------------------------
// LearnedFusion
// ---------------------------------------------------------------------------

/// Fusion using learned per-strategy weights (e.g. from an offline evaluation
/// run). Delegates to WeightedFusion with the provided learned weights.
pub struct LearnedFusion;

impl LearnedFusion {
    #[instrument(fields(strategy_count = strategy_results.len(), result_count))]
    pub fn fuse(
        strategy_results: Vec<(RetrievalStrategy, Vec<RetrievedChunk>)>,
        learned_weights: &[(String, f32)],
    ) -> Vec<RetrievedChunk> {
        let result = WeightedFusion::fuse(strategy_results, learned_weights);
        tracing::Span::current().record("result_count", result.len());
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_chunk(id_suffix: u8, score: f32, strategy: RetrievalStrategy) -> RetrievedChunk {
        use uuid::Uuid;
        // Build a deterministic UUID for easy dedup testing.
        let uuid = Uuid::parse_str(&format!(
            "00000000-0000-0000-0000-{:012x}", id_suffix as u64
        )).unwrap();
        RetrievedChunk {
            indexed_chunk: IndexedChunk {
                chunk: Chunk {
                    id: ChunkId(uuid),
                    text: format!("chunk_{}", id_suffix),
                    document_id: DocumentId::new(),
                    collection_id: CollectionId("col".into()),
                    position: ChunkPosition { start: 0, end: 1, index: 0 },
                    metadata: ChunkMetadata::default(),
                },
                vector: Vector(vec![]),
                token_vectors: None,
                store_id: "".into(),
            },
            score,
            strategy,
        }
    }

    #[test]
    fn test_weighted_fusion_higher_weight_scores_higher() {
        let chunks_vector = vec![make_chunk(1, 0.5, RetrievalStrategy::Vector)];
        let chunks_bm25   = vec![make_chunk(2, 0.5, RetrievalStrategy::Bm25)];

        let strategy_results = vec![
            (RetrievalStrategy::Vector, chunks_vector),
            (RetrievalStrategy::Bm25,   chunks_bm25),
        ];
        let weights = vec![
            ("Vector".to_string(), 2.0),
            ("Bm25".to_string(),   0.5),
        ];
        let result = WeightedFusion::fuse(strategy_results, &weights);
        assert_eq!(result.len(), 2);
        // Vector chunk: 0.5 * 2.0 = 1.0; Bm25 chunk: 0.5 * 0.5 = 0.25
        assert!(result[0].score > result[1].score,
            "Higher-weighted chunk should rank first");
        assert!((result[0].score - 1.0).abs() < 1e-5,
            "Vector chunk score should be 1.0");
    }

    #[test]
    fn test_learned_fusion_applies_weight() {
        let chunks = vec![make_chunk(1, 0.8, RetrievalStrategy::Vector)];
        let strategy_results = vec![(RetrievalStrategy::Vector, chunks)];
        let learned_weights = vec![("Vector".to_string(), 1.5)];
        let result = LearnedFusion::fuse(strategy_results, &learned_weights);
        assert_eq!(result.len(), 1);
        assert!((result[0].score - 1.2).abs() < 1e-5,
            "Learned weight 1.5 * 0.8 should give score 1.2");
    }

    #[test]
    fn test_weighted_fusion_dedup_keeps_max_score() {
        // Same chunk id, two strategies, different weights.
        let chunk_a = make_chunk(1, 0.6, RetrievalStrategy::Vector);
        let chunk_b = make_chunk(1, 0.6, RetrievalStrategy::Bm25); // same id!
        let strategy_results = vec![
            (RetrievalStrategy::Vector, vec![chunk_a]),
            (RetrievalStrategy::Bm25,   vec![chunk_b]),
        ];
        let weights = vec![
            ("Vector".to_string(), 2.0), // 0.6 * 2.0 = 1.2
            ("Bm25".to_string(),   0.5), // 0.6 * 0.5 = 0.3
        ];
        let result = WeightedFusion::fuse(strategy_results, &weights);
        assert_eq!(result.len(), 1, "Same chunk should be deduped to one entry");
        assert!((result[0].score - 1.2).abs() < 1e-5,
            "Should keep the higher weighted score");
    }
}
