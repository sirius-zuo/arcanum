//! Build per-sentence results with located evidence.

use crate::attribute::Attribution;
use crate::hydrate::HydratedPassage;
use crate::judge::{ClaimKind, JudgedSentence, Support};
use crate::quote::locate;
use crate::segment::Unit;
use crate::verdict::sentence_verdict;
use arcanum_core::types::{ClaimResult, Evidence, SentenceResult, SentenceVerdict};
use std::collections::HashMap;

fn evidence(support: &Support, passages: &[HydratedPassage]) -> Option<Evidence> {
    let p = passages.iter().find(|p| p.ref_id == support.ref_id)?;
    let located = locate(&p.text, &support.quote);
    let (start, end, chunk_id, quote, matched) = match located {
        Some((a, b)) => {
            let start = p.offset_start + a;
            (
                start,
                p.offset_start + b,
                p.chunk_at(start).clone(),
                p.text[a..b].to_string(),
                true,
            )
        }
        None => (
            p.offset_start,
            p.offset_end,
            p.chunks[0].0.clone(),
            support.quote.clone(),
            false,
        ),
    };
    Some(Evidence {
        ref_id: p.ref_id.clone(),
        chunk_id,
        document_id: p.document_id.clone(),
        version_num: p.version_num,
        version_status: p.version_status.clone(),
        source_uri: p.source_uri.clone(),
        offset_start: start,
        offset_end: end,
        quote,
        quote_matched: matched,
    })
}

pub fn build_sentences(
    answer: &str,
    units: &[Unit],
    attributions: &[Attribution],
    judged: &HashMap<usize, JudgedSentence>,
    passages: &[HydratedPassage],
) -> Vec<SentenceResult> {
    units
        .iter()
        .zip(attributions)
        .enumerate()
        .map(|(i, (unit, attr))| {
            let text = answer[unit.span.0..unit.span.1].to_string();
            let (verdict, claims) = if unit.code {
                (SentenceVerdict::NoClaim, vec![])
            } else if let Some(j) = judged.get(&(i + 1)) {
                let verdict = sentence_verdict(j.kind, &j.claims, &attr.cited);
                let claims = if j.kind == ClaimKind::NoClaim {
                    vec![]
                } else {
                    j.claims
                        .iter()
                        .map(|c| ClaimResult {
                            text: c.text.clone(),
                            supported: !c.support.is_empty(),
                            evidence: c
                                .support
                                .iter()
                                .filter_map(|s| evidence(s, passages))
                                .collect(),
                        })
                        .collect()
                };
                (verdict, claims)
            } else {
                (
                    SentenceVerdict::Unsupported,
                    vec![ClaimResult {
                        text: text.clone(),
                        supported: false,
                        evidence: vec![],
                    }],
                )
            };
            SentenceResult {
                span: unit.span,
                text,
                verdict,
                cited: attr.cited.clone(),
                invalid_refs: attr.invalid_refs.clone(),
                claims,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hydrate::join_chunks;
    use crate::judge::JudgedClaim;
    use arcanum_core::types::{ChunkBackend, ChunkId, ChunkMetadataRecord, DocumentId};

    const DOC: &str = "The quick brown fox jumps over the lazy dog.";

    fn rec(doc: &DocumentId, start: usize, end: usize) -> ChunkMetadataRecord {
        ChunkMetadataRecord {
            chunk_id: ChunkId::new(),
            document_id: doc.clone(),
            collection_id: "c".into(),
            version_num: 1,
            backend: ChunkBackend::Vector,
            text: DOC[start..end].to_string(),
            chunk_index: 0,
            source_uri: "raw://d".into(),
            snapshot_uri: "snap://d".into(),
            canonical_uri: None,
            page: None,
            section: None,
            block_ids: vec![],
            offset_start: start + 100,
            offset_end: end + 100,
            ingested_at: chrono::Utc::now(),
        }
    }

    fn passage() -> (HydratedPassage, ChunkId) {
        let d = DocumentId::new();
        let a = rec(&d, 0, 20);
        let first = a.chunk_id.clone();
        let mut p = join_chunks("P1", vec![a, rec(&d, 15, 44)]).unwrap();
        p.version_status = "active".into();
        (p, first)
    }

    fn judged_with(quote: &str) -> HashMap<usize, JudgedSentence> {
        let s = JudgedSentence {
            id: 1,
            kind: ClaimKind::Claim,
            claims: vec![JudgedClaim {
                text: "fox".into(),
                support: vec![Support {
                    ref_id: "P1".into(),
                    quote: quote.into(),
                }],
            }],
        };
        HashMap::from([(1, s)])
    }

    fn run(quote: &str) -> (SentenceResult, ChunkId) {
        let (p, first) = passage();
        let answer = "A fox [P1].";
        let units = vec![Unit {
            span: (0, answer.len()),
            code: false,
        }];
        let attrs = vec![Attribution {
            cited: vec!["P1".into()],
            invalid_refs: vec![],
        }];
        let mut out = build_sentences(answer, &units, &attrs, &judged_with(quote), &[p]);
        (out.remove(0), first)
    }

    #[test]
    fn build_sentences_evidence_offsets() {
        let (r, first) = run("brown fox jumps");
        assert_eq!(r.verdict, SentenceVerdict::Supported);
        assert!(r.claims[0].supported);
        let e = &r.claims[0].evidence[0];
        assert_eq!((e.offset_start, e.offset_end), (110, 125));
        assert_eq!(e.chunk_id, first);
        assert!(e.quote_matched);
        assert_eq!(e.quote, "brown fox jumps");
        assert_eq!((e.version_status.as_str(), e.version_num), ("active", 1));
        assert_eq!(e.source_uri, "raw://d");

        let (r, first) = run("purple");
        let e = &r.claims[0].evidence[0];
        assert_eq!((e.offset_start, e.offset_end), (100, 144));
        assert_eq!(e.chunk_id, first);
        assert!(!e.quote_matched);
        assert_eq!(e.quote, "purple");
    }

    #[test]
    fn build_sentences_code_and_unjudged_units() {
        let answer = "```\nx\n```\nHello there.";
        let units = vec![
            Unit {
                span: (0, 9),
                code: true,
            },
            Unit {
                span: (10, 22),
                code: false,
            },
        ];
        let attrs = vec![Attribution::default(), Attribution::default()];
        let out = build_sentences(answer, &units, &attrs, &HashMap::new(), &[]);
        assert_eq!(out[0].verdict, SentenceVerdict::NoClaim);
        assert!(out[0].claims.is_empty());
        assert_eq!(out[1].verdict, SentenceVerdict::Unsupported);
        assert_eq!(out[1].claims.len(), 1);
        assert_eq!(out[1].claims[0].text, "Hello there.");
        assert!(!out[1].claims[0].supported);
        assert!(out[1].claims[0].evidence.is_empty());
    }
}
