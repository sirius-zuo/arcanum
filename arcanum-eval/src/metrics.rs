use arcanum_core::types::ChunkId;
use arcanum_core::types::{EnrichRequest, EnrichIntent, EnrichedText};
use arcanum_core::traits::TextEnricher;
use arcanum_core::Result;
use std::collections::HashSet;
use std::sync::Arc;
use tracing::instrument;

#[instrument(skip(retrieved, relevant))]
pub fn compute_hit_rate_at_k(retrieved: &[ChunkId], relevant: &[ChunkId], k: usize) -> f32 {
    let top_k: HashSet<_> = retrieved.iter().take(k).map(|c| &c.0).collect();
    let hit = relevant.iter().any(|r| top_k.contains(&r.0));
    if hit { 1.0 } else { 0.0 }
}

#[instrument(skip(retrieved, relevant))]
pub fn compute_mrr(retrieved: &[ChunkId], relevant: &[ChunkId]) -> f32 {
    let rel_set: HashSet<_> = relevant.iter().map(|r| &r.0).collect();
    retrieved.iter().enumerate()
        .find(|(_, id)| rel_set.contains(&id.0))
        .map(|(rank, _)| 1.0 / (rank + 1) as f32)
        .unwrap_or(0.0)
}

#[instrument(skip(retrieved, relevant))]
pub fn compute_ndcg_at_k(retrieved: &[ChunkId], relevant: &[ChunkId], k: usize) -> f32 {
    let rel_set: HashSet<_> = relevant.iter().map(|r| &r.0).collect();
    let dcg: f32 = retrieved.iter().take(k).enumerate()
        .filter(|(_, id)| rel_set.contains(&id.0))
        .map(|(i, _)| 1.0 / (i as f32 + 2.0).log2())
        .sum();
    let ideal_dcg: f32 = (0..relevant.len().min(k))
        .map(|i| 1.0 / (i as f32 + 2.0).log2())
        .sum();
    if ideal_dcg == 0.0 { 0.0 } else { dcg / ideal_dcg }
}

#[instrument(skip(retrieved, relevant))]
pub fn compute_precision_at_k(retrieved: &[ChunkId], relevant: &[ChunkId], k: usize) -> f32 {
    if k == 0 { return 0.0; }
    let n = k.min(retrieved.len());
    if n == 0 { return 0.0; }
    let top_k: HashSet<_> = retrieved.iter().take(k).map(|c| &c.0).collect();
    let rel_set: HashSet<_> = relevant.iter().map(|c| &c.0).collect();
    let hits = top_k.intersection(&rel_set).count();
    hits as f32 / n as f32
}

#[instrument(skip(retrieved, relevant))]
pub fn compute_recall_at_k(retrieved: &[ChunkId], relevant: &[ChunkId], k: usize) -> f32 {
    if relevant.is_empty() { return 1.0; }
    let top_k: HashSet<_> = retrieved.iter().take(k).map(|c| &c.0).collect();
    let rel_set: HashSet<_> = relevant.iter().map(|c| &c.0).collect();
    let hits = top_k.intersection(&rel_set).count();
    hits as f32 / relevant.len() as f32
}

    #[instrument(skip(question, contexts, enricher), fields(question_len = question.len(), context_count = contexts.len()), err)]
    async fn compute_context_precision(
    question: &str,
    contexts: &[String],
    enricher: Arc<dyn TextEnricher>,
) -> Result<f32> {
    if contexts.is_empty() { return Ok(0.0); }
    let mut relevant_count = 0;
    for ctx in contexts {
        let req = EnrichRequest {
            text: format!("Is this context relevant to answering the question?\nQuestion: {}\nContext: {}\nAnswer with a number from 0 to 1:", question, ctx),
            intent: EnrichIntent::Custom("context_precision".to_string()),
            context: None,
        };
        if let Ok(EnrichedText(s)) = enricher.enrich(req).await {
            if s.trim().parse::<f32>().unwrap_or(0.0) >= 0.5 {
                relevant_count += 1;
            }
        }
    }
    Ok(relevant_count as f32 / contexts.len() as f32)
}

    #[instrument(skip(ground_truth_answer, contexts, enricher), fields(context_count = contexts.len()), err)]
    pub async fn compute_context_recall(
    ground_truth_answer: &str,
    contexts: &[String],
    enricher: Arc<dyn TextEnricher>,
) -> Result<f32> {
    if contexts.is_empty() { return Ok(0.0); }
    let joined = contexts.join("\n");
    let req = EnrichRequest {
        text: format!("Rate 0-1 how well these contexts support this answer. Answer only with a number.\nAnswer: {}\nContexts: {}", ground_truth_answer, joined),
        intent: EnrichIntent::Custom("context_recall".to_string()),
        context: None,
    };
    let score = enricher.enrich(req).await.ok()
        .and_then(|e| e.0.trim().parse::<f32>().ok())
        .unwrap_or(0.0);
    Ok(score.clamp(0.0, 1.0))
}

    #[instrument(skip(generated_answer, contexts, enricher), fields(context_count = contexts.len()), err)]
    pub async fn compute_faithfulness(
    generated_answer: &str,
    contexts: &[String],
    enricher: Arc<dyn TextEnricher>,
) -> Result<f32> {
    if contexts.is_empty() { return Ok(0.0); }
    let joined = contexts.join("\n");
    let req = EnrichRequest {
        text: format!("Rate 0-1 how faithfully this answer is supported by the contexts. Answer only with a number.\nAnswer: {}\nContexts: {}", generated_answer, joined),
        intent: EnrichIntent::Custom("faithfulness".to_string()),
        context: None,
    };
    let score = enricher.enrich(req).await.ok()
        .and_then(|e| e.0.trim().parse::<f32>().ok())
        .unwrap_or(0.0);
    Ok(score.clamp(0.0, 1.0))
}

    #[instrument(skip(question, generated_answer, enricher), fields(question_len = question.len()), err)]
    pub async fn compute_answer_relevance(
    question: &str,
    generated_answer: &str,
    enricher: Arc<dyn TextEnricher>,
) -> Result<f32> {
    let req = EnrichRequest {
        text: format!("Rate 0-1 how relevant this answer is to the question. Answer only with a number.\nQuestion: {}\nAnswer: {}", question, generated_answer),
        intent: EnrichIntent::Custom("answer_relevance".to_string()),
        context: None,
    };
    let score = enricher.enrich(req).await.ok()
        .and_then(|e| e.0.trim().parse::<f32>().ok())
        .unwrap_or(0.0);
    Ok(score.clamp(0.0, 1.0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use uuid::Uuid;

    fn ids(seeds: &[u128]) -> Vec<ChunkId> {
        seeds.iter().map(|&n| ChunkId(Uuid::from_u128(n))).collect()
    }

    struct YesEnricher;
    #[async_trait]
    impl TextEnricher for YesEnricher {
        async fn enrich(&self, _req: EnrichRequest) -> arcanum_core::Result<EnrichedText> {
            Ok(EnrichedText("1.0".to_string()))
        }
    }

    #[test]
    fn test_precision_at_k_perfect() {
        let retrieved = ids(&[1, 2, 3, 4, 5]);
        let relevant = ids(&[1, 2, 3]);
        assert!((compute_precision_at_k(&retrieved, &relevant, 3) - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_precision_at_k_partial() {
        let retrieved = ids(&[1, 2, 99, 100]);
        let relevant = ids(&[1, 2, 3]);
        assert!((compute_precision_at_k(&retrieved, &relevant, 4) - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_recall_at_k_full() {
        let retrieved = ids(&[1, 2, 3, 4, 5]);
        let relevant = ids(&[1, 2, 3]);
        assert!((compute_recall_at_k(&retrieved, &relevant, 5) - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_recall_at_k_partial() {
        let retrieved = ids(&[1, 99, 100]);
        let relevant = ids(&[1, 2, 3]);
        assert!((compute_recall_at_k(&retrieved, &relevant, 3) - 1.0/3.0).abs() < 0.01);
    }

    #[tokio::test]
    async fn test_context_precision_with_fake_enricher() {
        let score = compute_context_precision(
            "What is ML?",
            &["Machine learning is a subset of AI.".to_string()],
            Arc::new(YesEnricher),
        ).await.unwrap();
        assert!((score - 1.0).abs() < 0.01);
    }
}
