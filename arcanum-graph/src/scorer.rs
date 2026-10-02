use arcanum_core::{
    traits::{GraphQuery, GraphScorer, GraphStore},
    types::ChunkId,
    Result,
};
use async_trait::async_trait;
use std::collections::HashMap;

/// Scores chunks by graph proximity: each entity reachable from a seed
/// contributes `1 / (1 + hops)` to every chunk it was extracted from.
pub struct HopDecayScorer {
    max_hops: u32,
}

impl HopDecayScorer {
    pub fn new(max_hops: u32) -> Self {
        Self { max_hops }
    }
}

#[async_trait]
impl GraphScorer for HopDecayScorer {
    async fn score(
        &self,
        graph: &dyn GraphStore,
        collection: &str,
        seeds: &[String],
        limit: usize,
    ) -> Result<Vec<(ChunkId, f32)>> {
        let mut scores: HashMap<ChunkId, f32> = HashMap::new();
        for seed in seeds {
            let hits = graph
                .query(
                    collection,
                    &GraphQuery {
                        entity_name: Some(seed.clone()),
                        entity_type: None,
                        max_hops: self.max_hops,
                        relation_filter: None,
                    },
                )
                .await?;
            for hit in hits {
                let w = 1.0 / (1.0 + hit.hops as f32);
                for id in hit.entity.source_chunks {
                    *scores.entry(id).or_insert(0.0) += w;
                }
            }
        }
        let mut out: Vec<(ChunkId, f32)> = scores.into_iter().collect();
        out.sort_by(|a, b| {
            b.1.partial_cmp(&a.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.0 .0.to_string().cmp(&b.0 .0.to_string()))
        });
        out.truncate(limit);
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::InMemoryGraphStore;
    use arcanum_core::types::{Entity, EntityId, Relation};

    const COL: &str = "c";

    fn ent(name: &str, chunks: Vec<ChunkId>) -> Entity {
        Entity {
            id: EntityId::new(),
            name: name.into(),
            entity_type: "T".into(),
            canonical_id: None,
            source_chunks: chunks,
            source_uri: "u".into(),
            collection_id: COL.into(),
        }
    }

    #[tokio::test]
    async fn seed_only_scores_one() {
        let g = InMemoryGraphStore::new();
        let c1 = ChunkId::new();
        g.upsert_entities(COL, vec![ent("A", vec![c1.clone()])])
            .await
            .unwrap();
        let r = HopDecayScorer::new(0)
            .score(&g, COL, &["A".into()], 10)
            .await
            .unwrap();
        assert_eq!(r, vec![(c1, 1.0)]);
    }

    #[tokio::test]
    async fn multi_hop_decays() {
        let g = InMemoryGraphStore::new();
        let (c1, c2) = (ChunkId::new(), ChunkId::new());
        let a = ent("A", vec![c1.clone()]);
        let b = ent("B", vec![c2.clone()]);
        let rel = Relation {
            source: a.id.clone(),
            relation_type: "R".into(),
            target: b.id.clone(),
            confidence: 1.0,
            source_chunks: vec![],
        };
        g.upsert_entities(COL, vec![a, b]).await.unwrap();
        g.upsert_relations(COL, vec![rel]).await.unwrap();
        let r = HopDecayScorer::new(1)
            .score(&g, COL, &["A".into()], 10)
            .await
            .unwrap();
        assert_eq!(r, vec![(c1, 1.0), (c2, 0.5)]);
    }

    #[tokio::test]
    async fn contributions_sum_across_entities() {
        let g = InMemoryGraphStore::new();
        let c1 = ChunkId::new();
        g.upsert_entities(
            COL,
            vec![ent("A", vec![c1.clone()]), ent("B", vec![c1.clone()])],
        )
        .await
        .unwrap();
        let r = HopDecayScorer::new(0)
            .score(&g, COL, &["A".into(), "B".into()], 10)
            .await
            .unwrap();
        assert_eq!(r, vec![(c1, 2.0)]);
    }

    #[tokio::test]
    async fn limit_truncates_after_sorting() {
        let g = InMemoryGraphStore::new();
        let (c1, c2) = (ChunkId::new(), ChunkId::new());
        g.upsert_entities(
            COL,
            vec![
                ent("A", vec![c1.clone()]),
                ent("B", vec![c1.clone(), c2.clone()]),
            ],
        )
        .await
        .unwrap();
        let r = HopDecayScorer::new(0)
            .score(&g, COL, &["A".into(), "B".into()], 1)
            .await
            .unwrap();
        assert_eq!(r, vec![(c1, 2.0)]);
    }

    #[tokio::test]
    async fn unknown_seed_scores_nothing() {
        let g = InMemoryGraphStore::new();
        g.upsert_entities(COL, vec![ent("A", vec![ChunkId::new()])])
            .await
            .unwrap();
        let r = HopDecayScorer::new(2)
            .score(&g, COL, &["Nope".into()], 10)
            .await
            .unwrap();
        assert!(r.is_empty());
    }
}
