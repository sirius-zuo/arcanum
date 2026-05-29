use arcanum_eval::{compute_hit_rate_at_k, compute_mrr, compute_ndcg_at_k};
use arcanum_core::types::*;

#[test]
fn test_hit_rate_perfect_retrieval() {
    let relevant_id = ChunkId::new();
    let retrieved_ids = vec![relevant_id.clone()];
    let rate = compute_hit_rate_at_k(&retrieved_ids, &[relevant_id], 5);
    assert_eq!(rate, 1.0);
}

#[test]
fn test_hit_rate_miss() {
    let rate = compute_hit_rate_at_k(&[ChunkId::new()], &[ChunkId::new()], 5);
    assert_eq!(rate, 0.0);
}

#[test]
fn test_mrr_first_result_relevant() {
    let id = ChunkId::new();
    let mrr = compute_mrr(&[id.clone()], &[id]);
    assert_eq!(mrr, 1.0);
}

#[test]
fn test_mrr_second_result_relevant() {
    let id = ChunkId::new();
    let irrelevant = ChunkId::new();
    let mrr = compute_mrr(&[irrelevant, id.clone()], &[id]);
    assert!((mrr - 0.5).abs() < 0.001);
}
