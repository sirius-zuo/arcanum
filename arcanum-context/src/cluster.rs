use arcanum_core::types::*;
use std::collections::{BTreeMap, HashMap};

/// Reciprocal rank fusion constant, matching `search`.
pub const RRF_K: f64 = 60.0;

#[derive(Debug, Clone)]
pub struct Cluster {
    pub anchor: Chunk,
    pub chunk_ids: Vec<ChunkId>,
    pub strategies: Vec<&'static str>,
    pub score: f64,
    pub earliest_start: usize,
    pub section: Option<String>,
    pub page: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct ScoredSummary {
    pub chunk: Chunk,
    pub level: u32,
    pub covers: Vec<ChunkId>,
    pub score: f64,
}

fn rrf(rank: usize) -> f64 {
    1.0 / (RRF_K + rank as f64)
}

fn overlap(a: &ChunkPosition, b: &ChunkPosition) -> usize {
    a.end.min(b.end).saturating_sub(a.start.max(b.start))
}

/// Per-chunk record: first-seen chunk, summed RRF score, and every
/// `(list index, rank)` appearance.
struct Seen<'a> {
    chunk: &'a Chunk,
    score: f64,
    appearances: Vec<(usize, usize)>,
}

fn collect<'a>(
    lists: &'a [CandidateList],
    want_source: bool,
) -> (Vec<ChunkId>, HashMap<ChunkId, Seen<'a>>) {
    let mut order = Vec::new();
    let mut seen: HashMap<ChunkId, Seen<'a>> = HashMap::new();
    for (li, l) in lists.iter().enumerate() {
        for (i, rc) in l.chunks.iter().enumerate() {
            let is_source = matches!(rc.kind, ChunkKind::Source);
            if is_source != want_source {
                continue;
            }
            let chunk = &rc.indexed_chunk.chunk;
            if want_source && chunk.position.end <= chunk.position.start {
                continue;
            }
            let rank = i + 1;
            let e = seen.entry(chunk.id.clone()).or_insert_with(|| {
                order.push(chunk.id.clone());
                Seen {
                    chunk,
                    score: 0.0,
                    appearances: Vec::new(),
                }
            });
            e.score += rrf(rank);
            e.appearances.push((li, rank));
        }
    }
    (order, seen)
}

struct Building {
    anchor: ChunkId,
    members: Vec<ChunkId>,
}

pub fn cluster_sources(lists: &[CandidateList]) -> Vec<Cluster> {
    let (order, seen) = collect(lists, true);

    let mut buckets: HashMap<(DocumentId, u32), Vec<&ChunkId>> = HashMap::new();
    let mut bucket_order = Vec::new();
    for id in &order {
        let c = seen[id].chunk;
        let key = (c.document_id.clone(), c.provenance.document_version);
        if !buckets.contains_key(&key) {
            bucket_order.push(key.clone());
        }
        buckets.entry(key).or_default().push(id);
    }

    let mut clusters = Vec::new();
    for key in bucket_order {
        let mut ids = buckets.remove(&key).unwrap_or_default();
        ids.sort_by(|a, b| {
            let (sa, sb) = (&seen[*a], &seen[*b]);
            sb.score
                .total_cmp(&sa.score)
                .then(sa.chunk.position.start.cmp(&sb.chunk.position.start))
                .then(a.0.to_string().cmp(&b.0.to_string()))
        });
        let mut building: Vec<Building> = Vec::new();
        for id in ids {
            let pos = &seen[id].chunk.position;
            let len = pos.end - pos.start;
            let mut best: Option<(usize, usize)> = None;
            for (bi, b) in building.iter().enumerate() {
                let ov = overlap(pos, &seen[&b.anchor].chunk.position);
                if ov * 2 >= len && best.is_none_or(|(_, bo)| ov > bo) {
                    best = Some((bi, ov));
                }
            }
            match best {
                Some((bi, _)) => building[bi].members.push(id.clone()),
                None => building.push(Building {
                    anchor: id.clone(),
                    members: vec![id.clone()],
                }),
            }
        }
        for b in building {
            clusters.push(finish(&b, &seen, lists));
        }
    }

    clusters.sort_by(|a, b| {
        b.score
            .total_cmp(&a.score)
            .then(
                a.anchor
                    .document_id
                    .0
                    .to_string()
                    .cmp(&b.anchor.document_id.0.to_string()),
            )
            .then(a.anchor.position.start.cmp(&b.anchor.position.start))
            .then(a.anchor.id.0.to_string().cmp(&b.anchor.id.0.to_string()))
    });
    clusters
}

fn finish(b: &Building, seen: &HashMap<ChunkId, Seen<'_>>, lists: &[CandidateList]) -> Cluster {
    // Best (smallest) rank per list across all members.
    let mut best: BTreeMap<usize, usize> = BTreeMap::new();
    for id in &b.members {
        for &(li, rank) in &seen[id].appearances {
            best.entry(li)
                .and_modify(|r| *r = (*r).min(rank))
                .or_insert(rank);
        }
    }
    let score = best.values().map(|&r| rrf(r)).sum();
    let mut strategies: Vec<&'static str> = Vec::new();
    for &li in best.keys() {
        let name = strategy_name(&lists[li].strategy);
        if !strategies.contains(&name) {
            strategies.push(name);
        }
    }

    let earliest = b
        .members
        .iter()
        .map(|id| seen[id].chunk)
        .reduce(|a, c| {
            if c.position.start < a.position.start {
                c
            } else {
                a
            }
        })
        .expect("cluster has at least its anchor");

    Cluster {
        anchor: seen[&b.anchor].chunk.clone(),
        chunk_ids: b.members.clone(),
        strategies,
        score,
        earliest_start: earliest.position.start,
        section: earliest.provenance.section.clone(),
        page: earliest.provenance.page,
    }
}

pub fn score_summaries(lists: &[CandidateList]) -> Vec<ScoredSummary> {
    let (order, seen) = collect(lists, false);
    let mut out: Vec<ScoredSummary> = Vec::new();
    for id in order {
        let e = &seen[&id];
        // Level and covers come from the first list entry carrying this id.
        let (level, covers) = lists
            .iter()
            .flat_map(|l| l.chunks.iter())
            .find_map(|rc| match &rc.kind {
                ChunkKind::Summary { level, covers } if rc.indexed_chunk.chunk.id == id => {
                    Some((*level, covers.clone()))
                }
                _ => None,
            })
            .unwrap_or((0, Vec::new()));
        out.push(ScoredSummary {
            chunk: e.chunk.clone(),
            level,
            covers,
            score: e.score,
        });
    }
    out.sort_by(|a, b| {
        b.score
            .total_cmp(&a.score)
            .then(a.chunk.id.0.to_string().cmp(&b.chunk.id.0.to_string()))
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{list, src, summary};

    const DOC: &str = "0123456789012345678901234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234567890123456789";

    fn v(chunks: Vec<Chunk>) -> Vec<CandidateList> {
        vec![list(RetrievalStrategy::Vector, 0, chunks)]
    }

    #[test]
    fn chunk_joins_anchor_at_exactly_half_overlap() {
        let d = DocumentId::new();
        let cl = cluster_sources(&v(vec![src(&d, 1, DOC, 0, 100), src(&d, 1, DOC, 50, 150)]));
        assert_eq!(cl.len(), 1);
        assert_eq!(cl[0].chunk_ids.len(), 2);
    }

    #[test]
    fn chunk_below_half_overlap_becomes_new_anchor() {
        let d = DocumentId::new();
        let cl = cluster_sources(&v(vec![src(&d, 1, DOC, 0, 100), src(&d, 1, DOC, 51, 151)]));
        assert_eq!(cl.len(), 2);
    }

    #[test]
    fn large_chunk_containing_small_anchor_is_not_absorbed() {
        let d = DocumentId::new();
        let cl = cluster_sources(&v(vec![src(&d, 1, DOC, 40, 60), src(&d, 1, DOC, 0, 200)]));
        assert_eq!(cl.len(), 2);
    }

    #[test]
    fn ties_join_the_larger_overlap() {
        let d = DocumentId::new();
        let a1 = src(&d, 1, DOC, 0, 100);
        let a2 = src(&d, 1, DOC, 100, 200);
        let c = src(&d, 1, DOC, 40, 180);
        let a2_id = a2.id.clone();
        let cl = cluster_sources(&v(vec![a1, a2, c]));
        assert_eq!(cl.len(), 2);
        let second = cl.iter().find(|x| x.anchor.id == a2_id).unwrap();
        assert_eq!(second.chunk_ids.len(), 2);
    }

    #[test]
    fn different_versions_never_cluster() {
        let d = DocumentId::new();
        let cl = cluster_sources(&v(vec![src(&d, 1, DOC, 0, 100), src(&d, 2, DOC, 0, 100)]));
        assert_eq!(cl.len(), 2);
    }

    #[test]
    fn zero_length_and_inverted_spans_are_skipped() {
        let d = DocumentId::new();
        let mut inverted = src(&d, 1, DOC, 0, 10);
        inverted.position.start = 20;
        inverted.position.end = 5;
        let cl = cluster_sources(&v(vec![src(&d, 1, DOC, 10, 10), inverted]));
        assert!(cl.is_empty());
    }

    #[test]
    fn best_rank_per_list_counts_once() {
        let d = DocumentId::new();
        let cl = cluster_sources(&v(vec![src(&d, 1, DOC, 0, 100), src(&d, 1, DOC, 50, 150)]));
        assert_eq!(cl.len(), 1);
        assert!((cl[0].score - 1.0 / 61.0).abs() < 1e-12);
    }

    #[test]
    fn cross_list_agreement_adds_per_list() {
        let d = DocumentId::new();
        let filler = src(&d, 1, DOC, 150, 190);
        let filler2 = src(&d, 1, DOC, 190, 199);
        let a = src(&d, 1, DOC, 0, 100);
        let b = src(&d, 1, DOC, 50, 150);
        let lists = vec![
            list(RetrievalStrategy::Vector, 0, vec![filler, a]),
            list(
                RetrievalStrategy::Bm25,
                0,
                vec![filler2.clone(), filler2, b],
            ),
        ];
        let cl = cluster_sources(&lists);
        let big = cl.iter().find(|c| c.chunk_ids.len() == 2).unwrap();
        assert!((big.score - (1.0 / 62.0 + 1.0 / 63.0)).abs() < 1e-12);
        assert_eq!(big.strategies, vec!["vector", "bm25"]);
    }

    #[test]
    fn three_backends_outrank_single_rank_one() {
        let d = DocumentId::new();
        let x = src(&d, 1, DOC, 0, 20);
        let y = src(&d, 1, DOC, 100, 120);
        let x_id = x.id.clone();
        let filler = |s: usize| (s..s + 2).map(|i| src(&d, 1, DOC, 30 + i * 12, 40 + i * 12));
        let mut lists = Vec::new();
        for s in [
            RetrievalStrategy::Vector,
            RetrievalStrategy::Bm25,
            RetrievalStrategy::Graph,
        ] {
            let mut chunks: Vec<Chunk> = filler(lists.len() * 3).collect();
            chunks.push(x.clone());
            lists.push(list(s, 0, chunks));
        }
        lists[0].chunks.insert(
            0,
            list(RetrievalStrategy::Vector, 0, vec![y]).chunks.remove(0),
        );
        lists[0].chunks.pop();
        lists[0].chunks.push(
            list(RetrievalStrategy::Vector, 0, vec![x.clone()])
                .chunks
                .remove(0),
        );
        // x: rank 4 in vector (after y insert), rank 3 in bm25 and graph; y: rank 1 in vector only
        let cl = cluster_sources(&lists);
        assert_eq!(cl[0].anchor.id, x_id);
        assert_eq!(cl[0].strategies, vec!["vector", "bm25", "graph"]);
    }

    #[test]
    fn same_strategy_twice_listed_once_counted_twice() {
        let d = DocumentId::new();
        let c = src(&d, 1, DOC, 0, 50);
        let lists = vec![
            list(RetrievalStrategy::Vector, 0, vec![c.clone()]),
            list(RetrievalStrategy::Vector, 1, vec![c]),
        ];
        let cl = cluster_sources(&lists);
        assert_eq!(cl.len(), 1);
        assert_eq!(cl[0].strategies, vec!["vector"]);
        assert!((cl[0].score - 2.0 / 61.0).abs() < 1e-12);
    }

    #[test]
    fn summaries_scored_separately_not_clustered() {
        let d = DocumentId::new();
        let s = summary(&d, 1, "overview text", vec![]);
        let sid = s.indexed_chunk.chunk.id.clone();
        let lists = vec![CandidateList {
            query_index: 0,
            strategy: RetrievalStrategy::Raptor,
            chunks: vec![s],
        }];
        assert!(cluster_sources(&lists).is_empty());
        let ss = score_summaries(&lists);
        assert_eq!(ss.len(), 1);
        assert_eq!(ss[0].chunk.id, sid);
        assert_eq!(ss[0].level, 1);
        assert!((ss[0].score - 1.0 / 61.0).abs() < 1e-12);
    }

    #[test]
    fn earliest_member_supplies_section_and_page() {
        let d = DocumentId::new();
        let a = src(&d, 1, DOC, 20, 120);
        let mut b = src(&d, 1, DOC, 0, 40);
        b.provenance.section = Some("Intro".into());
        b.provenance.page = Some(2);
        // b has 20/40 = 50% overlap with a, joins a as the lower-ranked member
        let cl = cluster_sources(&v(vec![a, b]));
        assert_eq!(cl.len(), 1);
        assert_eq!(cl[0].earliest_start, 0);
        assert_eq!(cl[0].section.as_deref(), Some("Intro"));
        assert_eq!(cl[0].page, Some(2));
    }
}
