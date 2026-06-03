use arcanum_core::{
    traits::{Evaluator, TextEnricher, EvalMetrics, GroundTruth},
    types::{ChunkId, Query, RetrievedChunk},
    Result,
};
use async_trait::async_trait;
use std::sync::Arc;
use crate::metrics::{compute_hit_rate_at_k, compute_mrr, compute_ndcg_at_k};
use tracing::instrument;
use metrics;

pub struct StandardEvaluator {
    pub enricher: Arc<dyn TextEnricher>,
    pub k: usize,
}

impl StandardEvaluator {
    pub fn new(enricher: Arc<dyn TextEnricher>, k: usize) -> Self {
        Self { enricher, k }
    }
}

#[async_trait]
impl Evaluator for StandardEvaluator {
    #[instrument(skip(self, results, ground_truths), fields(num_results = results.len(), ground_truth_count = ground_truths.len(), k = self.k), err)]
    async fn evaluate(
        &self,
        results: &[(Query, Vec<RetrievedChunk>)],
        ground_truths: &[GroundTruth],
    ) -> Result<EvalMetrics> {
        let result = do_evaluate(self, results, ground_truths).await;
        let status = if result.is_ok() { "ok" } else { "error" };
        metrics::counter!("arcanum_eval_runs_total", "metric" => "standard", "status" => status).increment(1);
        result
    }
}

async fn do_evaluate(
    state: &StandardEvaluator,
    results: &[(Query, Vec<RetrievedChunk>)],
    ground_truths: &[GroundTruth],
) -> Result<EvalMetrics> {
        let n = results.len() as f32;
        if n == 0.0 {
            return Ok(EvalMetrics { hit_rate_at_k: 0.0, mrr: 0.0, ndcg_at_k: 0.0, k: state.k });
        }
        let mut hr = 0f32;
        let mut mrr = 0f32;
        let mut ndcg = 0f32;
        for ((_, chunks), gt) in results.iter().zip(ground_truths.iter()) {
            let retrieved_ids: Vec<ChunkId> = chunks.iter()
                .map(|c| c.indexed_chunk.chunk.id.clone())
                .collect();
            hr   += compute_hit_rate_at_k(&retrieved_ids, &gt.relevant_chunk_ids, state.k);
            mrr  += compute_mrr(&retrieved_ids, &gt.relevant_chunk_ids);
            ndcg += compute_ndcg_at_k(&retrieved_ids, &gt.relevant_chunk_ids, state.k);
        }
        Ok(EvalMetrics {
            hit_rate_at_k: hr / n,
            mrr: mrr / n,
            ndcg_at_k: ndcg / n,
            k: state.k,
        })
    }

#[cfg(test)]
mod tests {
    use super::*;
    use arcanum_core::{
        traits::TextEnricher,
        types::{EnrichRequest, EnrichedText, Query},
    };
    use async_trait::async_trait;

    struct FakeEnricher;
    #[async_trait]
    impl TextEnricher for FakeEnricher {
        async fn enrich(&self, _req: EnrichRequest) -> arcanum_core::Result<EnrichedText> {
            Ok(EnrichedText("0.5".to_string()))
        }
    }

    #[tokio::test]
    async fn test_standard_evaluator_empty() {
        let evaluator = StandardEvaluator::new(Arc::new(FakeEnricher), 5);
        let metrics = evaluator.evaluate(&[], &[]).await.unwrap();
        assert_eq!(metrics.hit_rate_at_k, 0.0);
        assert_eq!(metrics.mrr, 0.0);
        assert_eq!(metrics.ndcg_at_k, 0.0);
        assert_eq!(metrics.k, 5);
    }
}
