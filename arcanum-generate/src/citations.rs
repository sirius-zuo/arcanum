use arcanum_core::types::context::Passage;
use arcanum_core::types::generate::Citation;
use regex::Regex;
use std::sync::OnceLock;

#[derive(Debug, Clone, PartialEq)]
pub struct ParsedCitations {
    pub citations: Vec<Citation>,
    pub unknown_refs: Vec<String>,
}

fn group_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"\[\s*[PS]\d{1,3}(?:\s*,\s*[PS]\d{1,3})*\s*\]").expect("valid regex")
    })
}

fn id_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"[PS]\d{1,3}").expect("valid regex"))
}

/// Scans for citation markers in the answer string and returns the byte span
/// and IDs for each marker group. Duplicates are preserved in the ID list.
pub fn scan_markers(answer: &str) -> Vec<((usize, usize), Vec<String>)> {
    let mut result = Vec::new();

    for group in group_re().find_iter(answer) {
        let span = (group.start(), group.end());
        let ids: Vec<String> = id_re()
            .find_iter(group.as_str())
            .map(|m| m.as_str().into())
            .collect();
        result.push((span, ids));
    }

    result
}

/// Maps inline `[P1]` / `[P2, P3]` markers back to passages. Spans are byte
/// ranges of the whole marker group; the answer is never modified.
pub fn parse_citations(answer: &str, passages: &[Passage]) -> ParsedCitations {
    let mut citations: Vec<Citation> = Vec::new();
    let mut unknown_refs: Vec<String> = Vec::new();

    for (span, ids) in scan_markers(answer) {
        for id in ids {
            if let Some(existing) = citations.iter_mut().find(|c| c.ref_id == id) {
                existing.answer_spans.push(span);
            } else if let Some(p) = passages.iter().find(|p| p.ref_id == id) {
                citations.push(Citation {
                    ref_id: p.ref_id.clone(),
                    chunk_ids: p.chunk_ids.clone(),
                    document_id: p.document_id.clone(),
                    version_num: p.version_num,
                    source_uri: p.source_uri.clone(),
                    offset_start: p.offset_start,
                    offset_end: p.offset_end,
                    answer_spans: vec![span],
                });
            } else if !unknown_refs.iter().any(|r| r == &id) {
                unknown_refs.push(id);
            }
        }
    }

    ParsedCitations {
        citations,
        unknown_refs,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arcanum_core::types::document::{ChunkId, DocumentId};
    use uuid::Uuid;

    fn passage(ref_id: &str, offset_start: usize) -> Passage {
        Passage {
            ref_id: ref_id.into(),
            document_id: DocumentId(Uuid::new_v4()),
            version_num: 2,
            source_uri: format!("src://{ref_id}"),
            snapshot_uri: "snap".into(),
            canonical_uri: None,
            section: None,
            page: None,
            offset_start,
            offset_end: offset_start + 10,
            text: "t".into(),
            chunk_ids: vec![ChunkId(Uuid::new_v4())],
            strategies: vec![],
            score: 1.0,
        }
    }

    fn by_ref<'a>(p: &'a ParsedCitations, id: &str) -> &'a Citation {
        p.citations.iter().find(|c| c.ref_id == id).unwrap()
    }

    #[test]
    fn single_and_adjacent_groups() {
        let answer = "Acme prices per kilogram [P1], with rideshare discounts [P2][P3].";
        let passages = vec![passage("P1", 100), passage("P2", 200)];
        let p = parse_citations(answer, &passages);
        assert_eq!(p.citations.len(), 2);
        assert_eq!(p.citations[0].ref_id, "P1");
        assert_eq!(p.citations[0].answer_spans, vec![(25, 29)]);
        assert_eq!(p.citations[1].ref_id, "P2");
        assert_eq!(p.citations[1].answer_spans, vec![(56, 60)]);
        assert_eq!(p.unknown_refs, vec!["P3".to_string()]);
        let c = &p.citations[0];
        assert_eq!(c.document_id, passages[0].document_id);
        assert_eq!(c.offset_start, 100);
        assert_eq!(c.chunk_ids, passages[0].chunk_ids);
    }

    #[test]
    fn spaced_group_shares_span_with_multibyte_prefix() {
        let answer = "café [P1] and [P2, P3]";
        let passages = vec![passage("P1", 1), passage("P2", 2), passage("P3", 3)];
        let p = parse_citations(answer, &passages);
        assert_eq!(by_ref(&p, "P1").answer_spans, vec![(6, 10)]);
        assert_eq!(by_ref(&p, "P2").answer_spans, vec![(15, 23)]);
        assert_eq!(by_ref(&p, "P3").answer_spans, vec![(15, 23)]);
        assert_eq!(&answer[6..10], "[P1]");
    }

    #[test]
    fn repeated_ref_collects_spans_in_order() {
        let passages = vec![passage("P1", 1), passage("P2", 2)];
        let p = parse_citations("[P2] a [P1] b [P2]", &passages);
        let order: Vec<_> = p.citations.iter().map(|c| c.ref_id.as_str()).collect();
        assert_eq!(order, vec!["P2", "P1"]);
        assert_eq!(by_ref(&p, "P2").answer_spans, vec![(0, 4), (14, 18)]);
    }

    #[test]
    fn summaries_and_missing_go_to_unknown_deduplicated() {
        let p = parse_citations("x [S1] y [P9] z [S1]", &[passage("P1", 1)]);
        assert!(p.citations.is_empty());
        assert_eq!(p.unknown_refs, vec!["S1".to_string(), "P9".to_string()]);
    }

    #[test]
    fn non_markers_are_plain_text() {
        let p = parse_citations("[p1] [P1-P3] [Source 1] [P1234]", &[passage("P1", 1)]);
        assert!(p.citations.is_empty());
        assert!(p.unknown_refs.is_empty());
    }

    #[test]
    fn scan_markers_returns_group_spans_and_ids() {
        assert_eq!(
            scan_markers("a [P1, S2] b [P3] [P1,P1]"),
            vec![
                ((2, 10), vec!["P1".into(), "S2".into()]),
                ((13, 17), vec!["P3".into()]),
                ((18, 25), vec!["P1".into(), "P1".into()])
            ]
        );
        assert!(scan_markers("no markers [X1]").is_empty());
    }
}
