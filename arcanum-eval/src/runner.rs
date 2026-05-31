use arcanum_core::types::*;
use crate::metrics::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoldenSample {
    pub query: String,
    pub relevant_chunk_ids: Vec<ChunkId>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalReport {
    pub hit_rate_at_k: f32,
    pub mrr: f32,
    pub ndcg_at_k: f32,
    pub k: usize,
    pub num_queries: usize,
    pub precision_at_k: f32,
    pub recall_at_k: f32,
    pub context_precision: Option<f32>,
    pub context_recall: Option<f32>,
    pub faithfulness: Option<f32>,
    pub answer_relevance: Option<f32>,
}

pub struct EvalRunner { pub k: usize }

impl EvalRunner {
    pub fn new(k: usize) -> Self { Self { k } }

    pub fn evaluate(&self, results: &[Vec<ChunkId>], ground_truths: &[GoldenSample]) -> EvalReport {
        assert_eq!(results.len(), ground_truths.len());
        let n = results.len() as f32;
        let mut hr = 0f32; let mut mrr = 0f32; let mut ndcg = 0f32;
        let mut precision = 0f32; let mut recall = 0f32;
        for (retrieved, gt) in results.iter().zip(ground_truths.iter()) {
            hr        += compute_hit_rate_at_k(retrieved, &gt.relevant_chunk_ids, self.k);
            mrr       += compute_mrr(retrieved, &gt.relevant_chunk_ids);
            ndcg      += compute_ndcg_at_k(retrieved, &gt.relevant_chunk_ids, self.k);
            precision += compute_precision_at_k(retrieved, &gt.relevant_chunk_ids, self.k);
            recall    += compute_recall_at_k(retrieved, &gt.relevant_chunk_ids, self.k);
        }
        EvalReport {
            hit_rate_at_k: hr / n,
            mrr: mrr / n,
            ndcg_at_k: ndcg / n,
            k: self.k,
            num_queries: results.len(),
            precision_at_k: precision / n,
            recall_at_k: recall / n,
            context_precision: None,
            context_recall: None,
            faithfulness: None,
            answer_relevance: None,
        }
    }
}
