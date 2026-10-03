//! Judge prompt, user message, and validation of the judge's JSON output.

use arcanum_context::render::render;
use arcanum_core::types::{Passage, RenderFormat};
use serde::Deserialize;
use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq)]
pub struct JudgeSentence {
    pub id: usize,
    pub text: String,
    pub cited: Vec<String>,
}

pub const JUDGE_SYSTEM_PROMPT: &str = r#"You check whether each sentence of an answer is supported by the passages given.
Rules:
1. Use only the passages given. Do not use outside knowledge.
2. For each numbered sentence, decide whether it makes factual claims. Greetings, transitions, hedges and statements that no information was found are "no_claim".
3. Split each claim sentence into its atomic claims, written in the sentence's language. Do not skip any part of the sentence.
4. For each claim, list every passage that supports it on its own. Support means the passage states or directly entails the claim. Partial overlap is not support.
5. For each support, copy the shortest verbatim span of the passage that shows it.
6. Citations in a sentence are hints only. Check all passages.
7. Output only JSON of this form, with one entry per sentence:
{"sentences":[{"id":1,"kind":"claim","claims":[{"text":"...","support":[{"ref":"P1","quote":"..."}]}]},{"id":2,"kind":"no_claim","claims":[]}]}"#;

fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            c => out.push(c),
        }
    }
    out
}

/// One `<sentence>` line of the user message; the single owner of that format.
pub fn sentence_fragment(s: &JudgeSentence) -> String {
    format!(
        "<sentence id=\"{}\" cited=\"{}\">{}</sentence>\n",
        s.id,
        esc(&s.cited.join(",")),
        esc(&s.text)
    )
}

pub fn user_message(passages: &[Passage], sentences: &[JudgeSentence]) -> String {
    let mut out = render(RenderFormat::Xml, passages, &[]);
    out.push_str("<sentences>\n");
    for s in sentences {
        out.push_str(&sentence_fragment(s));
    }
    out.push_str("</sentences>\n");
    out
}

pub fn retry_message(error: &str) -> String {
    format!("Your previous reply was invalid: {error}. Reply again with only the corrected JSON.")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClaimKind {
    Claim,
    NoClaim,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Support {
    #[serde(rename = "ref")]
    pub ref_id: String,
    #[serde(default)]
    pub quote: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct JudgedClaim {
    pub text: String,
    #[serde(default)]
    pub support: Vec<Support>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct JudgedSentence {
    pub id: usize,
    pub kind: ClaimKind,
    #[serde(default)]
    pub claims: Vec<JudgedClaim>,
}

#[derive(Deserialize)]
struct Output {
    sentences: Vec<JudgedSentence>,
}

/// Strips one surrounding code fence, then falls back to the first `{` .. last `}` span.
fn extract_json(raw: &str) -> &str {
    let original = raw.trim();
    let mut t = original;
    if t.starts_with("```") {
        let body = t.split_once('\n').map_or("", |(_, rest)| rest);
        t = body.trim_end().strip_suffix("```").unwrap_or(body).trim();
        if !t.starts_with('{') {
            // Single-line fence such as ```{...}``` or ```json{...}```.
            t = original;
        }
    }
    if !t.starts_with('{') {
        if let (Some(a), Some(b)) = (t.find('{'), t.rfind('}')) {
            if a < b {
                return &t[a..=b];
            }
        }
    }
    t
}

pub fn parse_judge_output(
    raw: &str,
    truncated: bool,
    batch_ids: &[usize],
    available: &[String],
) -> Result<Vec<JudgedSentence>, String> {
    if truncated {
        return Err("output truncated".into());
    }
    let out: Output = serde_json::from_str(extract_json(raw))
        .map_err(|e| format!("output is not valid JSON: {e}"))?;
    let mut seen = HashSet::new();
    for s in &out.sentences {
        if !batch_ids.contains(&s.id) {
            return Err(format!("unknown sentence {}", s.id));
        }
        if !seen.insert(s.id) {
            return Err(format!("duplicate sentence {}", s.id));
        }
    }
    for id in batch_ids {
        if !seen.contains(id) {
            return Err(format!("missing sentence {id}"));
        }
    }
    for s in &out.sentences {
        if s.kind == ClaimKind::Claim && s.claims.is_empty() {
            return Err(format!("sentence {} is a claim with no claims", s.id));
        }
        for sup in s.claims.iter().flat_map(|c| &c.support) {
            if !available.contains(&sup.ref_id) {
                return Err(format!("unknown passage ref {}", sup.ref_id));
            }
        }
    }
    Ok(out.sentences)
}

#[cfg(test)]
mod tests {
    use super::*;
    use arcanum_core::types::DocumentId;

    pub(crate) fn passage(r: &str, text: &str) -> Passage {
        Passage {
            ref_id: r.into(),
            document_id: DocumentId::new(),
            version_num: 1,
            source_uri: "raw://a".into(),
            snapshot_uri: String::new(),
            canonical_uri: None,
            section: None,
            page: None,
            offset_start: 0,
            offset_end: 0,
            text: text.into(),
            chunk_ids: vec![],
            strategies: vec![],
            score: 0.0,
        }
    }

    const VALID: &str = r#"{"sentences":[{"id":1,"kind":"claim","claims":[{"text":"c","support":[{"ref":"P1","quote":"q"}]}]},{"id":2,"kind":"no_claim","claims":[]}]}"#;

    fn avail() -> Vec<String> {
        vec!["P1".into()]
    }

    #[test]
    fn user_message_renders_passages_then_sentences() {
        let ps = [passage("P1", "text")];
        let ss = [
            JudgeSentence {
                id: 1,
                text: "A <b> & \"c\"".into(),
                cited: vec!["P1".into(), "P2".into()],
            },
            JudgeSentence {
                id: 2,
                text: "x".into(),
                cited: vec![],
            },
        ];
        let m = user_message(&ps, &ss);
        assert!(m.contains("<passage ref=\"P1\">"));
        assert!(m.contains(
            "<sentence id=\"1\" cited=\"P1,P2\">A &lt;b&gt; &amp; &quot;c&quot;</sentence>"
        ));
        assert!(m.contains("<sentence id=\"2\" cited=\"\">"));
        assert!(m.find("<documents>").unwrap() < m.find("<sentences>").unwrap());
    }

    #[test]
    fn system_prompt_is_spec_copy() {
        assert!(JUDGE_SYSTEM_PROMPT.contains("Use only the passages given"));
        assert!(JUDGE_SYSTEM_PROMPT.contains("Citations in a sentence are hints only"));
    }

    #[test]
    fn parse_accepts_plain_fenced_and_prose_wrapped() {
        let plain = parse_judge_output(VALID, false, &[1, 2], &avail()).unwrap();
        let fenced = format!("```json\n{VALID}\n```");
        let wrapped = format!("Here is the result: {VALID} Done.");
        assert_eq!(plain.len(), 2);
        assert_eq!(
            parse_judge_output(&fenced, false, &[1, 2], &avail()).unwrap(),
            plain
        );
        assert_eq!(
            parse_judge_output(&wrapped, false, &[1, 2], &avail()).unwrap(),
            plain
        );
    }

    #[test]
    fn parse_accepts_single_line_fence() {
        let plain = parse_judge_output(VALID, false, &[1, 2], &avail()).unwrap();
        for raw in [format!("```{VALID}```"), format!("```json{VALID}```")] {
            assert_eq!(
                parse_judge_output(&raw, false, &[1, 2], &avail()).unwrap(),
                plain
            );
        }
    }

    #[test]
    fn parse_rejects_each_invalid_case() {
        let a = avail();
        let err =
            |raw: &str, t: bool, ids: &[usize]| parse_judge_output(raw, t, ids, &a).unwrap_err();
        assert_eq!(err(VALID, true, &[1, 2]), "output truncated");
        assert!(err("not json", false, &[1, 2]).starts_with("output is not valid JSON"));
        assert_eq!(err(VALID, false, &[1]), "unknown sentence 2");
        let dup = r#"{"sentences":[{"id":1,"kind":"no_claim"},{"id":1,"kind":"no_claim"}]}"#;
        assert_eq!(err(dup, false, &[1]), "duplicate sentence 1");
        let one = r#"{"sentences":[{"id":1,"kind":"no_claim"}]}"#;
        assert_eq!(err(one, false, &[1, 2]), "missing sentence 2");
        let empty = r#"{"sentences":[{"id":1,"kind":"claim","claims":[]}]}"#;
        assert_eq!(
            err(empty, false, &[1]),
            "sentence 1 is a claim with no claims"
        );
        let bad = VALID.replace("\"P1\"", "\"P7\"");
        assert_eq!(err(&bad, false, &[1, 2]), "unknown passage ref P7");
    }
}
