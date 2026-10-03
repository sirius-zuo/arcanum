use super::document::{ChunkId, DocumentId};
use super::generate::GeneratorInfo;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PassageRef {
    pub ref_id: String,
    pub chunk_ids: Vec<ChunkId>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VerifyRequest {
    pub collection_id: String,
    pub answer: String,
    pub passages: Vec<PassageRef>,
    pub judge: Option<String>,
    #[serde(default)]
    pub strict_citations: bool,
}

/// True for `^P\d{1,3}$`.
fn is_passage_ref_id(id: &str) -> bool {
    match id.strip_prefix('P') {
        Some(d) => (1..=3).contains(&d.len()) && d.bytes().all(|b| b.is_ascii_digit()),
        None => false,
    }
}

impl VerifyRequest {
    pub fn validate(
        &self,
        max_answer_chars: usize,
        max_passages: usize,
    ) -> std::result::Result<(), String> {
        if self.answer.trim().is_empty() {
            return Err("answer must not be empty".into());
        }
        if self.answer.chars().count() > max_answer_chars {
            return Err(format!(
                "answer must be at most {max_answer_chars} characters"
            ));
        }
        if self.passages.is_empty() {
            return Err("passages must not be empty".into());
        }
        if self.passages.len() > max_passages {
            return Err(format!("at most {max_passages} passages are allowed"));
        }
        let mut seen = std::collections::HashSet::new();
        for p in &self.passages {
            if !is_passage_ref_id(&p.ref_id) {
                return Err(format!("invalid ref_id: {}", p.ref_id));
            }
            if !seen.insert(p.ref_id.as_str()) {
                return Err(format!("duplicate ref_id: {}", p.ref_id));
            }
            if p.chunk_ids.is_empty() {
                return Err(format!("passage {} has no chunk_ids", p.ref_id));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OverallVerdict {
    Pass,
    Fail,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SentenceVerdict {
    Supported,
    Miscited,
    UncitedSupported,
    Partial,
    Unsupported,
    NoClaim,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct VerdictCounts {
    pub supported: usize,
    pub miscited: usize,
    pub uncited_supported: usize,
    pub partial: usize,
    pub unsupported: usize,
    pub no_claim: usize,
}

impl VerdictCounts {
    pub fn add(&mut self, v: SentenceVerdict) {
        match v {
            SentenceVerdict::Supported => self.supported += 1,
            SentenceVerdict::Miscited => self.miscited += 1,
            SentenceVerdict::UncitedSupported => self.uncited_supported += 1,
            SentenceVerdict::Partial => self.partial += 1,
            SentenceVerdict::Unsupported => self.unsupported += 1,
            SentenceVerdict::NoClaim => self.no_claim += 1,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Evidence {
    pub ref_id: String,
    pub chunk_id: ChunkId,
    pub document_id: DocumentId,
    pub version_num: u32,
    pub version_status: String,
    pub source_uri: String,
    pub offset_start: usize,
    pub offset_end: usize,
    pub quote: String,
    pub quote_matched: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClaimResult {
    pub text: String,
    pub supported: bool,
    pub evidence: Vec<Evidence>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SentenceResult {
    pub span: (usize, usize),
    pub text: String,
    pub verdict: SentenceVerdict,
    pub cited: Vec<String>,
    pub invalid_refs: Vec<String>,
    pub claims: Vec<ClaimResult>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VerifyUsage {
    pub input_tokens: Option<u32>,
    pub output_tokens: Option<u32>,
    pub judge_calls: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VerifyResponse {
    pub verdict: OverallVerdict,
    pub strict_citations: bool,
    pub counts: VerdictCounts,
    pub sentences: Vec<SentenceResult>,
    pub passages_unavailable: Vec<String>,
    pub judge: GeneratorInfo,
    pub usage: VerifyUsage,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum Verification {
    Ok(VerifyResponse),
    Error { code: String, message: String },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{DocumentId, GeneratorInfo};
    use serde_json::json;

    type Case = (fn(&mut VerifyRequest), &'static str);

    fn chunk() -> ChunkId {
        ChunkId(uuid::Uuid::nil())
    }

    fn base() -> VerifyRequest {
        VerifyRequest {
            collection_id: "c".into(),
            answer: "a".into(),
            passages: vec![PassageRef {
                ref_id: "P1".into(),
                chunk_ids: vec![chunk()],
            }],
            judge: None,
            strict_citations: false,
        }
    }

    fn sample_response() -> VerifyResponse {
        let mut counts = VerdictCounts::default();
        counts.add(SentenceVerdict::UncitedSupported);
        VerifyResponse {
            verdict: OverallVerdict::Pass,
            strict_citations: false,
            counts,
            sentences: vec![SentenceResult {
                span: (0, 4),
                text: "Acme".into(),
                verdict: SentenceVerdict::UncitedSupported,
                cited: vec![],
                invalid_refs: vec![],
                claims: vec![ClaimResult {
                    text: "Acme".into(),
                    supported: true,
                    evidence: vec![Evidence {
                        ref_id: "P1".into(),
                        chunk_id: chunk(),
                        document_id: DocumentId(uuid::Uuid::nil()),
                        version_num: 1,
                        version_status: "active".into(),
                        source_uri: "u".into(),
                        offset_start: 0,
                        offset_end: 4,
                        quote: "Acme".into(),
                        quote_matched: true,
                    }],
                }],
            }],
            passages_unavailable: vec![],
            judge: GeneratorInfo {
                name: "cheap".into(),
                model: "m".into(),
            },
            usage: VerifyUsage {
                input_tokens: Some(1),
                output_tokens: None,
                judge_calls: 1,
            },
        }
    }

    #[test]
    fn validate_follows_spec_table() {
        let cases: Vec<Case> = vec![
            (|r| r.answer = " \n".into(), "answer must not be empty"),
            (
                |r| r.answer = "é".repeat(11),
                "answer must be at most 10 characters",
            ),
            (|r| r.passages.clear(), "passages must not be empty"),
            (
                |r| {
                    let p = r.passages[0].clone();
                    r.passages = vec![p; 3]
                        .into_iter()
                        .enumerate()
                        .map(|(i, mut p)| {
                            p.ref_id = format!("P{}", i + 1);
                            p
                        })
                        .collect()
                },
                "at most 2 passages are allowed",
            ),
            (|r| r.passages[0].ref_id = "S1".into(), "invalid ref_id: S1"),
            (
                |r| {
                    let p = r.passages[0].clone();
                    r.passages.push(p)
                },
                "duplicate ref_id: P1",
            ),
            (
                |r| r.passages[0].chunk_ids.clear(),
                "passage P1 has no chunk_ids",
            ),
        ];
        for (mutate, msg) in cases {
            let mut r = base();
            mutate(&mut r);
            assert_eq!(r.validate(10, 2).unwrap_err(), msg);
        }
        assert!(base().validate(10, 2).is_ok());
        let mut r = base();
        r.answer = "é".repeat(10);
        assert!(r.validate(10, 2).is_ok());
        r.passages[0].ref_id = "P1000".into();
        assert_eq!(r.validate(10, 2).unwrap_err(), "invalid ref_id: P1000");
    }

    #[test]
    fn request_accepts_context_passages_unchanged() {
        let id = uuid::Uuid::nil().to_string();
        let r: VerifyRequest = serde_json::from_value(json!({
            "collection_id": "c",
            "answer": "x [P1]",
            "passages": [{
                "ref_id": "P1",
                "document_id": id,
                "version_num": 1,
                "source_uri": "u",
                "snapshot_uri": "s",
                "canonical_uri": null,
                "section": null,
                "page": null,
                "offset_start": 0,
                "offset_end": 5,
                "text": "hello",
                "chunk_ids": [id],
                "strategies": ["dense"],
                "score": 0.5
            }]
        }))
        .unwrap();
        assert_eq!(r.passages.len(), 1);
        assert_eq!(r.passages[0].ref_id, "P1");
        assert_eq!(r.passages[0].chunk_ids, vec![chunk()]);
        assert!(!r.strict_citations);
        assert_eq!(r.judge, None);
    }

    #[test]
    fn verification_is_tagged_by_status() {
        let v = serde_json::to_value(Verification::Ok(sample_response())).unwrap();
        assert_eq!(v["status"], "ok");
        assert_eq!(v["verdict"], "pass");
        assert_eq!(v["sentences"][0]["verdict"], "uncited_supported");
        assert_eq!(v["sentences"][0]["span"], json!([0, 4]));
        let e = Verification::Error {
            code: "judge_timeout".into(),
            message: "judge timed out".into(),
        };
        assert_eq!(
            serde_json::to_value(e).unwrap(),
            json!({"status": "error", "code": "judge_timeout", "message": "judge timed out"})
        );
    }

    #[test]
    fn counts_add_increments_matching_bucket() {
        let mut c = VerdictCounts::default();
        c.add(SentenceVerdict::Supported);
        c.add(SentenceVerdict::NoClaim);
        c.add(SentenceVerdict::NoClaim);
        assert_eq!((c.supported, c.no_claim, c.partial), (1, 2, 0));
    }
}
