use async_trait::async_trait;
use crate::types::*;
use crate::Result;

#[async_trait]
pub trait Retriever: Send + Sync {
    async fn retrieve(&self, query: &Query) -> Result<Vec<RetrievedChunk>>;
    fn strategy(&self) -> RetrievalStrategy;
}

#[async_trait]
pub trait Reranker: Send + Sync {
    async fn rerank(&self, query: &Query, chunks: Vec<RetrievedChunk>) -> Result<Vec<RetrievedChunk>>;
}

#[derive(Debug, Clone)]
pub struct GroundTruth {
    pub query: Query,
    pub relevant_chunk_ids: Vec<ChunkId>,
    pub expected_answer: Option<String>,
}

#[derive(Debug, Clone)]
pub struct EvalMetrics {
    pub hit_rate_at_k: f32,
    pub mrr: f32,
    pub ndcg_at_k: f32,
    pub k: usize,
}

#[async_trait]
pub trait Evaluator: Send + Sync {
    async fn evaluate(
        &self,
        results: &[(Query, Vec<RetrievedChunk>)],
        ground_truths: &[GroundTruth],
    ) -> Result<EvalMetrics>;
}

#[cfg(test)]
mod tests {
    use super::*;

    struct PassthroughReranker;
    #[async_trait]
    impl Reranker for PassthroughReranker {
        async fn rerank(&self, _q: &Query, chunks: Vec<RetrievedChunk>) -> Result<Vec<RetrievedChunk>> {
            Ok(chunks)
        }
    }

    #[tokio::test]
    async fn test_reranker_passthrough() {
        let r = PassthroughReranker;
        let q = Query::new("test");
        let result = r.rerank(&q, vec![]).await.unwrap();
        assert!(result.is_empty());
    }
}
