use arcanum_core::types::ChunkId;

pub fn compute_hit_rate_at_k(retrieved: &[ChunkId], relevant: &[ChunkId], k: usize) -> f32 {
    let top_k: std::collections::HashSet<_> = retrieved.iter().take(k).map(|c| &c.0).collect();
    let hit = relevant.iter().any(|r| top_k.contains(&r.0));
    if hit { 1.0 } else { 0.0 }
}

pub fn compute_mrr(retrieved: &[ChunkId], relevant: &[ChunkId]) -> f32 {
    let rel_set: std::collections::HashSet<_> = relevant.iter().map(|r| &r.0).collect();
    retrieved.iter().enumerate()
        .find(|(_, id)| rel_set.contains(&id.0))
        .map(|(rank, _)| 1.0 / (rank + 1) as f32)
        .unwrap_or(0.0)
}

pub fn compute_ndcg_at_k(retrieved: &[ChunkId], relevant: &[ChunkId], k: usize) -> f32 {
    let rel_set: std::collections::HashSet<_> = relevant.iter().map(|r| &r.0).collect();
    let dcg: f32 = retrieved.iter().take(k).enumerate()
        .filter(|(_, id)| rel_set.contains(&id.0))
        .map(|(i, _)| 1.0 / (i as f32 + 2.0).log2())
        .sum();
    let ideal_dcg: f32 = (0..relevant.len().min(k))
        .map(|i| 1.0 / (i as f32 + 2.0).log2())
        .sum();
    if ideal_dcg == 0.0 { 0.0 } else { dcg / ideal_dcg }
}
