use arcanum_core::traits::TokenCounter;
use arcanum_core::types::*;

use crate::cluster::{cluster_sources, score_summaries, Cluster, ScoredSummary};
use crate::render::{
    background_wrapper, document_wrapper, documents_envelope, passage_fragment, render,
    summary_fragment,
};

#[derive(Debug, Clone)]
pub struct AssembleParams {
    pub token_budget: usize,
    pub background_share: f32,
    pub render: Option<RenderFormat>,
}

#[derive(Debug, Clone)]
pub struct Assembled {
    pub passages: Vec<Passage>,
    pub background: Vec<BackgroundItem>,
    pub usage: ContextUsage,
    pub rendered: Option<String>,
}

/// A selected (possibly merged) passage with the number of clusters it holds.
struct Selected {
    passage: Passage,
    clusters: usize,
    earliest_start: usize,
}

fn passage_from(c: &Cluster) -> Selected {
    let a = &c.anchor;
    Selected {
        passage: Passage {
            ref_id: String::new(),
            document_id: a.document_id.clone(),
            version_num: a.provenance.document_version,
            source_uri: a.provenance.source_uri.clone(),
            snapshot_uri: a.provenance.snapshot_uri.clone(),
            canonical_uri: a.provenance.canonical_uri.clone(),
            section: c.section.clone(),
            page: c.page,
            offset_start: a.position.start,
            offset_end: a.position.end,
            text: a.text.clone(),
            chunk_ids: c.chunk_ids.clone(),
            strategies: c.strategies.iter().map(|s| s.to_string()).collect(),
            score: c.score,
        },
        clusters: 1,
        earliest_start: c.earliest_start,
    }
}

fn background_from(s: &ScoredSummary) -> BackgroundItem {
    BackgroundItem {
        ref_id: String::new(),
        text: s.chunk.text.clone(),
        level: s.level,
        document_id: s.chunk.document_id.clone(),
        covers: s.covers.clone(),
        score: s.score,
    }
}

fn count_nonempty(counter: &dyn TokenCounter, s: &str) -> usize {
    if s.is_empty() {
        0
    } else {
        counter.count(s)
    }
}

/// Merges `next` into `cur` when they overlap or touch (spec 5.4 step 3).
/// Both texts should be slices of one document, so `cur.offset_end` is a
/// char boundary of `next.text` at the computed index. When a bad registry
/// row breaks that (text shorter than its span, or an index off a char
/// boundary), `next` is returned unmerged instead of panicking.
fn absorb(cur: &mut Selected, next: Selected) -> Option<Selected> {
    if next.passage.offset_end > cur.passage.offset_end {
        let idx = cur.passage.offset_end - next.passage.offset_start;
        let Some(tail) = next.passage.text.get(idx..) else {
            return Some(next);
        };
        cur.passage.text.push_str(tail);
        cur.passage.offset_end = next.passage.offset_end;
    }
    let (c, n) = (&mut cur.passage, next.passage);
    for id in n.chunk_ids {
        if !c.chunk_ids.contains(&id) {
            c.chunk_ids.push(id);
        }
    }
    for s in n.strategies {
        if !c.strategies.contains(&s) {
            c.strategies.push(s);
        }
    }
    c.score = c.score.max(n.score);
    if next.earliest_start < cur.earliest_start {
        c.section = n.section;
        c.page = n.page;
        cur.earliest_start = next.earliest_start;
    }
    cur.clusters += next.clusters;
    None
}

fn merge(selected: Vec<Selected>) -> Vec<Selected> {
    let mut groups: Vec<((DocumentId, u32), Vec<Selected>)> = Vec::new();
    for s in selected {
        let key = (s.passage.document_id.clone(), s.passage.version_num);
        match groups.iter_mut().find(|(k, _)| *k == key) {
            Some((_, g)) => g.push(s),
            None => groups.push((key, vec![s])),
        }
    }
    let mut out = Vec::new();
    for (_, mut g) in groups {
        g.sort_by_key(|s| (s.passage.offset_start, s.passage.offset_end));
        let mut iter = g.into_iter();
        let mut cur = iter.next().expect("groups are non-empty");
        for next in iter {
            let next = if next.passage.offset_start <= cur.passage.offset_end {
                match absorb(&mut cur, next) {
                    None => continue,
                    Some(unmerged) => unmerged,
                }
            } else {
                next
            };
            out.push(cur);
            cur = next;
        }
        out.push(cur);
    }
    out
}

/// Orders documents by best passage score, passages by offset, and assigns
/// ref ids (spec 5.4 step 5). `background` is already in score order.
fn order_and_number(passages: &mut [Selected], background: &mut [BackgroundItem]) {
    let key = |s: &Selected| (s.passage.document_id.clone(), s.passage.version_num);
    let mut best: Vec<((DocumentId, u32), f64)> = Vec::new();
    for s in passages.iter() {
        let k = key(s);
        match best.iter_mut().find(|(bk, _)| *bk == k) {
            Some((_, b)) => *b = b.max(s.passage.score),
            None => best.push((k, s.passage.score)),
        }
    }
    let best_of = |s: &Selected| {
        let k = key(s);
        best.iter()
            .find(|(bk, _)| *bk == k)
            .map_or(0.0, |(_, b)| *b)
    };
    passages.sort_by(|a, b| {
        best_of(b).total_cmp(&best_of(a)).then_with(|| {
            let (pa, pb) = (&a.passage, &b.passage);
            pa.document_id
                .0
                .cmp(&pb.document_id.0)
                .then(pa.version_num.cmp(&pb.version_num))
                .then(pa.offset_start.cmp(&pb.offset_start))
        })
    });
    for (i, s) in passages.iter_mut().enumerate() {
        s.passage.ref_id = format!("P{}", i + 1);
    }
    for (i, b) in background.iter_mut().enumerate() {
        b.ref_id = format!("S{}", i + 1);
    }
}

pub fn assemble(
    candidates: &Candidates,
    params: &AssembleParams,
    counter: &dyn TokenCounter,
) -> Assembled {
    let fmt = params.render;
    let budget = params.token_budget;

    // 1. Background, within its share. Costs use the widest ref id (S999)
    // and the background wrapper is paid by the first summary selected.
    let allowance = ((budget as f32 * params.background_share).floor() as usize).min(budget);
    let mut background: Vec<BackgroundItem> = Vec::new();
    let mut bg_used = 0;
    for s in score_summaries(&candidates.lists) {
        let mut item = background_from(&s);
        item.ref_id = "S999".into();
        let mut cost = counter.count(&summary_fragment(fmt, &item));
        if background.is_empty() {
            cost += count_nonempty(counter, &background_wrapper(fmt));
        }
        if bg_used + cost <= allowance {
            bg_used += cost;
            background.push(item);
        }
    }

    // 2. Passages, in what the background left. The envelope is paid by
    // the first passage overall, each document wrapper by its first passage.
    let allowance = budget - bg_used;
    let mut selected: Vec<Selected> = Vec::new();
    let mut wrapped: Vec<(DocumentId, u32)> = Vec::new();
    let mut used = 0;
    let mut dropped = 0;
    for c in cluster_sources(&candidates.lists) {
        let mut s = passage_from(&c);
        s.passage.ref_id = "P999".into();
        let mut cost = counter.count(&passage_fragment(fmt, &s.passage));
        if selected.is_empty() {
            cost += count_nonempty(counter, &documents_envelope(fmt));
        }
        let key = (s.passage.document_id.clone(), s.passage.version_num);
        let new_doc = !wrapped.contains(&key);
        if new_doc {
            cost += count_nonempty(
                counter,
                &document_wrapper(fmt, &s.passage.source_uri, s.passage.version_num),
            );
        }
        if used + cost <= allowance {
            used += cost;
            if new_doc {
                wrapped.push(key);
            }
            selected.push(s);
        } else {
            dropped += 1;
        }
    }

    // 3. Merge.
    let mut passages = merge(selected);

    // 4 and 5. Order, number and re-count; drop the lowest-scoring passage
    // (then summary, once no passage is left) until the output fits.
    loop {
        order_and_number(&mut passages, &mut background);
        let ps: Vec<Passage> = passages.iter().map(|s| s.passage.clone()).collect();
        let (p_cost, b_cost) = item_costs(fmt, &ps, &background, counter);
        let rendered = fmt.map(|f| render(f, &ps, &background));
        let total = match &rendered {
            Some(r) => count_nonempty(counter, r),
            None => p_cost + b_cost,
        };
        if total <= budget {
            return Assembled {
                passages: ps,
                background,
                usage: ContextUsage {
                    budget,
                    used: total,
                    passages: p_cost,
                    background: b_cost,
                    dropped_passages: dropped,
                    counter: counter.name().to_string(),
                },
                rendered,
            };
        }
        if let Some(i) = lowest(passages.iter().map(|s| s.passage.score)) {
            dropped += passages.remove(i).clusters;
        } else if let Some(i) = lowest(background.iter().map(|b| b.score)) {
            background.remove(i);
        } else {
            unreachable!("empty output counts zero tokens");
        }
    }
}

/// Index of the lowest score; ties go to the later item.
fn lowest(scores: impl Iterator<Item = f64>) -> Option<usize> {
    scores
        .enumerate()
        .reduce(|lo, x| if x.1 <= lo.1 { x } else { lo })
        .map(|(i, _)| i)
}

/// Sums of fragment plus wrapper costs for the final passages and summaries.
fn item_costs(
    fmt: Option<RenderFormat>,
    passages: &[Passage],
    background: &[BackgroundItem],
    counter: &dyn TokenCounter,
) -> (usize, usize) {
    let mut p = 0;
    if !passages.is_empty() {
        p += count_nonempty(counter, &documents_envelope(fmt));
    }
    let mut prev: Option<(&str, u32)> = None;
    for x in passages {
        let key = (x.source_uri.as_str(), x.version_num);
        if prev != Some(key) {
            p += count_nonempty(counter, &document_wrapper(fmt, key.0, key.1));
            prev = Some(key);
        }
        p += counter.count(&passage_fragment(fmt, x));
    }
    let mut b = 0;
    if !background.is_empty() {
        b += count_nonempty(counter, &background_wrapper(fmt));
    }
    for x in background {
        b += counter.count(&summary_fragment(fmt, x));
    }
    (p, b)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{list, src, summary, WordCounter};

    const DOC: &str = "0123456789012345678901234567890123456789012345678901234567890123456789012345678901234567890123456789";

    fn words(prefix: &str, n: usize) -> String {
        (0..n)
            .map(|i| format!("{prefix}{i}"))
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// A one-chunk document whose chunk spans the whole text.
    fn whole(text: &str) -> Chunk {
        src(&DocumentId::new(), 1, text, 0, text.len())
    }

    fn cands(lists: Vec<CandidateList>) -> Candidates {
        Candidates {
            queries: vec![],
            lists,
            failed: vec![],
        }
    }

    fn vector(chunks: Vec<Chunk>) -> CandidateList {
        list(RetrievalStrategy::Vector, 0, chunks)
    }

    fn raptor(items: Vec<RetrievedChunk>) -> CandidateList {
        CandidateList {
            query_index: 0,
            strategy: RetrievalStrategy::Raptor,
            chunks: items,
        }
    }

    fn params(budget: usize, share: f32) -> AssembleParams {
        AssembleParams {
            token_budget: budget,
            background_share: share,
            render: None,
        }
    }

    #[test]
    fn never_exceeds_budget() {
        let chunks: Vec<Chunk> = (0..40).map(|i| whole(&words("w", 5 + i * 3))).collect();
        let d = DocumentId::new();
        let bg: Vec<RetrievedChunk> = (0..5)
            .map(|i| summary(&d, 1, &words("s", 4 + i * 7), vec![]))
            .collect();
        let c = cands(vec![vector(chunks), raptor(bg)]);
        for render in [
            None,
            Some(RenderFormat::Numbered),
            Some(RenderFormat::Xml),
            Some(RenderFormat::Markdown),
        ] {
            let mut p = params(200, 0.2);
            p.render = render;
            let out = assemble(&c, &p, &WordCounter);
            assert!(out.usage.used <= 200, "{render:?}: {}", out.usage.used);
            assert!(!out.passages.is_empty());
            assert_eq!(out.usage.budget, 200);
            assert_eq!(out.usage.counter, "words");
        }
    }

    #[test]
    fn skip_and_continue_takes_smaller_later_cluster() {
        let a = whole(&words("a", 50));
        let b = whole(&words("b", 300));
        let cc = whole(&words("c", 50));
        let (a_doc, c_doc) = (a.document_id.clone(), cc.document_id.clone());
        let out = assemble(
            &cands(vec![vector(vec![a, b, cc])]),
            &params(200, 0.0),
            &WordCounter,
        );
        let docs: Vec<_> = out.passages.iter().map(|p| p.document_id.clone()).collect();
        assert_eq!(docs, vec![a_doc, c_doc]);
        assert_eq!(out.usage.dropped_passages, 1);
        assert_eq!(out.usage.used, 100);
    }

    #[test]
    fn unused_background_rolls_over_to_passages() {
        let d = DocumentId::new();
        let c = cands(vec![
            vector(vec![whole(&words("p", 190))]),
            raptor(vec![summary(&d, 1, &words("s", 10), vec![])]),
        ]);
        let out = assemble(&c, &params(200, 0.5), &WordCounter);
        assert_eq!(out.background.len(), 1);
        assert_eq!(out.passages.len(), 1);
        assert_eq!(out.usage.background, 10);
        assert_eq!(out.usage.passages, 190);
        assert_eq!(out.usage.used, 200);
    }

    #[test]
    fn zero_share_selects_no_summaries() {
        let d = DocumentId::new();
        let c = cands(vec![
            vector(vec![whole(&words("p", 20))]),
            raptor(vec![summary(&d, 1, &words("s", 10), vec![])]),
        ]);
        let out = assemble(&c, &params(200, 0.0), &WordCounter);
        assert!(out.background.is_empty());
        assert_eq!(out.usage.background, 0);
        assert_eq!(out.passages.len(), 1);
    }

    #[test]
    fn document_wrapper_paid_once_per_document() {
        // Two non-touching 10-word spans of one document.
        let doc = format!("{} {}", words("a", 10), words("b", 10));
        let d = DocumentId::new();
        let mid = doc.find("b0").unwrap();
        let first = src(&d, 1, &doc, 0, mid - 1);
        let second = src(&d, 1, &doc, mid, doc.len());
        let c = cands(vec![vector(vec![first, second])]);
        // Xml word costs: envelope 2, document wrapper 4, each passage 11.
        // Wrapper once: 2 + 4 + 11 + 11 = 28. Wrapper twice would be 32.
        let mut p = params(30, 0.0);
        p.render = Some(RenderFormat::Xml);
        let out = assemble(&c, &p, &WordCounter);
        assert_eq!(out.passages.len(), 2);
        assert_eq!(out.usage.dropped_passages, 0);
        let rendered = out.rendered.as_deref().unwrap();
        assert_eq!(out.usage.used, WordCounter.count(rendered));
        assert_eq!(out.usage.used, 28);
        assert_eq!(out.usage.passages, 28);
    }

    #[test]
    fn overlapping_and_touching_passages_merge_to_slice() {
        let d = DocumentId::new();
        let a = src(&d, 1, DOC, 0, 40);
        let b = src(&d, 1, DOC, 30, 80);
        let e = src(&d, 1, DOC, 80, 100);
        let ids = vec![a.id.clone(), b.id.clone(), e.id.clone()];
        let c = cands(vec![vector(vec![a, b, e])]);
        let out = assemble(&c, &params(200, 0.0), &WordCounter);
        assert_eq!(out.passages.len(), 1);
        let p = &out.passages[0];
        assert_eq!(p.text, DOC[0..100]);
        assert_eq!((p.offset_start, p.offset_end), (0, 100));
        let mut got = p.chunk_ids.clone();
        got.sort_by_key(|x| x.0);
        let mut want = ids;
        want.sort_by_key(|x| x.0);
        assert_eq!(got, want);
        assert!((p.score - 1.0 / 61.0).abs() < 1e-12);
        assert_eq!(p.ref_id, "P1");
    }

    #[test]
    fn degenerate_merge_keeps_passages_separate_without_panic() {
        // Chunk text shorter than its span: the merge index is out of range.
        let d = DocumentId::new();
        let a = src(&d, 1, DOC, 0, 40);
        let mut b = src(&d, 1, DOC, 30, 80);
        b.text = "short".into();
        let out = assemble(
            &cands(vec![vector(vec![a, b])]),
            &params(200, 0.0),
            &WordCounter,
        );
        assert_eq!(out.passages.len(), 2);

        // Merge index lands inside a multi-byte char of the next chunk.
        let d = DocumentId::new();
        let a = src(&d, 1, DOC, 0, 10);
        let mut b = src(&d, 1, DOC, 9, 20);
        b.text = "\u{e9}xxxxxxxxx".into();
        let out = assemble(
            &cands(vec![vector(vec![a, b])]),
            &params(200, 0.0),
            &WordCounter,
        );
        assert_eq!(out.passages.len(), 2);
    }

    #[test]
    fn merged_passage_takes_section_from_earliest_cluster() {
        let d = DocumentId::new();
        let mut late = src(&d, 1, DOC, 30, 80);
        late.provenance.section = Some("Late".into());
        let mut early = src(&d, 1, DOC, 0, 40);
        early.provenance.section = Some("Early".into());
        early.provenance.page = Some(3);
        let out = assemble(
            &cands(vec![vector(vec![late, early])]),
            &params(200, 0.0),
            &WordCounter,
        );
        assert_eq!(out.passages.len(), 1);
        assert_eq!(out.passages[0].section.as_deref(), Some("Early"));
        assert_eq!(out.passages[0].page, Some(3));
        assert_eq!(out.passages[0].text, DOC[0..80]);
    }

    #[test]
    fn contained_passage_adds_no_text() {
        let d = DocumentId::new();
        // The small span ranks first, so the large one is not absorbed.
        let small = src(&d, 1, DOC, 20, 40);
        let big = src(&d, 1, DOC, 0, 100);
        let out = assemble(
            &cands(vec![vector(vec![small, big])]),
            &params(200, 0.0),
            &WordCounter,
        );
        assert_eq!(out.passages.len(), 1);
        assert_eq!(out.passages[0].text, DOC[0..100]);
        assert_eq!(out.passages[0].chunk_ids.len(), 2);
    }

    /// Joined output costs 10 extra per additional passage separator.
    struct SepCounter;
    impl TokenCounter for SepCounter {
        fn count(&self, text: &str) -> usize {
            text.split_whitespace().count() + 10 * text.matches("\n\n").count().saturating_sub(1)
        }
        fn name(&self) -> &'static str {
            "sep"
        }
    }

    #[test]
    fn final_check_drops_lowest_scoring_merged_passage() {
        let top = whole(&words("t", 10));
        let top_doc = top.document_id.clone();
        let doc = words("x", 20);
        let d = DocumentId::new();
        let mid = doc.find("x10").unwrap();
        let low1 = src(&d, 1, &doc, 0, mid);
        let low2 = src(&d, 1, &doc, mid, doc.len());
        // Packing: each Numbered fragment is 3 header words + 10 text words.
        let mut p = params(39, 0.0);
        p.render = Some(RenderFormat::Numbered);
        let out = assemble(&cands(vec![vector(vec![top, low1, low2])]), &p, &SepCounter);
        assert_eq!(out.passages.len(), 1);
        assert_eq!(out.passages[0].document_id, top_doc);
        assert_eq!(out.passages[0].ref_id, "P1");
        assert_eq!(out.usage.dropped_passages, 2);
        assert_eq!(out.usage.used, 13);
        assert_eq!(
            out.usage.used,
            SepCounter.count(out.rendered.as_deref().unwrap())
        );
    }

    #[test]
    fn final_check_falls_back_to_summaries() {
        // Every item fits on its own but each extra separator costs 10.
        let d = DocumentId::new();
        let c = cands(vec![raptor(vec![
            summary(&d, 1, &words("s", 5), vec![]),
            summary(&d, 1, &words("r", 5), vec![]),
        ])]);
        let mut p = params(18, 1.0);
        p.render = Some(RenderFormat::Markdown);
        let out = assemble(&c, &p, &SepCounter);
        assert!(out.usage.used <= 18, "{}", out.usage.used);
        assert_eq!(out.background.len(), 1);
        assert!(out.background[0].text.starts_with("s0"));
    }

    #[test]
    fn ordering_and_ref_ids() {
        let b = whole(&words("b", 5));
        let b_doc = b.document_id.clone();
        let doc = format!("{} {}", words("x", 5), words("y", 5));
        let a_doc = DocumentId::new();
        let mid = doc.find("y0").unwrap();
        let a_late = src(&a_doc, 1, &doc, mid, doc.len());
        let a_early = src(&a_doc, 1, &doc, 0, mid - 1);
        let sd = DocumentId::new();
        let c = cands(vec![
            vector(vec![b, a_late, a_early]),
            raptor(vec![
                summary(&sd, 1, "first summary", vec![]),
                summary(&sd, 2, "second summary", vec![]),
            ]),
        ]);
        let out = assemble(&c, &params(200, 0.2), &WordCounter);
        let ids: Vec<_> = out
            .passages
            .iter()
            .map(|p| (p.ref_id.as_str(), p.document_id.clone(), p.offset_start))
            .collect();
        assert_eq!(
            ids,
            vec![
                ("P1", b_doc, 0),
                ("P2", a_doc.clone(), 0),
                ("P3", a_doc, mid)
            ]
        );
        let s: Vec<_> = out
            .background
            .iter()
            .map(|s| (s.ref_id.as_str(), s.text.as_str(), s.level))
            .collect();
        assert_eq!(
            s,
            vec![("S1", "first summary", 1), ("S2", "second summary", 2)]
        );
    }

    #[test]
    fn nothing_fits_returns_empty_with_drops() {
        let out = assemble(
            &cands(vec![vector(vec![whole(&words("w", 500))])]),
            &params(200, 0.2),
            &WordCounter,
        );
        assert!(out.passages.is_empty());
        assert_eq!(out.usage.dropped_passages, 1);
        assert_eq!(out.usage.used, 0);
    }

    #[test]
    fn tiny_budget_keeps_background_drops_passages() {
        let d = DocumentId::new();
        let c = cands(vec![
            vector(vec![whole(&words("p", 300))]),
            raptor(vec![summary(&d, 1, &words("s", 20), vec![])]),
        ]);
        let mut p = params(200, 0.2);
        p.render = Some(RenderFormat::Xml);
        let out = assemble(&c, &p, &WordCounter);
        assert_eq!(out.background.len(), 1);
        assert!(out.usage.background <= 40);
        assert!(out.passages.is_empty());
        assert!(out.usage.dropped_passages > 0);
        assert!(out.usage.used <= 200);
        assert_eq!(
            out.usage.used,
            WordCounter.count(out.rendered.as_deref().unwrap())
        );
    }

    #[test]
    fn versions_never_merge() {
        let d = DocumentId::new();
        let c = cands(vec![vector(vec![
            src(&d, 1, DOC, 0, 50),
            src(&d, 2, DOC, 0, 50),
        ])]);
        let out = assemble(&c, &params(200, 0.0), &WordCounter);
        assert_eq!(out.passages.len(), 2);
        let mut v: Vec<u32> = out.passages.iter().map(|p| p.version_num).collect();
        v.sort();
        assert_eq!(v, vec![1, 2]);
    }

    #[test]
    fn rendered_matches_render_of_final_items() {
        let d = DocumentId::new();
        let c = cands(vec![
            vector(vec![whole(&words("p", 10)), whole(&words("q", 10))]),
            raptor(vec![summary(&d, 1, &words("s", 5), vec![])]),
        ]);
        let mut p = params(200, 0.2);
        p.render = Some(RenderFormat::Markdown);
        let out = assemble(&c, &p, &WordCounter);
        assert_eq!(
            out.rendered.as_deref().unwrap(),
            render(RenderFormat::Markdown, &out.passages, &out.background)
        );
        assert_eq!(out.usage.used, out.usage.passages + out.usage.background);
        assert!(assemble(&c, &params(200, 0.2), &WordCounter)
            .rendered
            .is_none());
    }
}
