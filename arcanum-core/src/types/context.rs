use super::document::{ChunkId, DocumentId, RetrievalStrategy, RetrievedChunk};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Assistant,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RenderFormat {
    Numbered,
    Xml,
    Markdown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextRequest {
    pub collection_id: String,
    pub query: Option<String>,
    pub messages: Option<Vec<Message>>,
    pub token_budget: Option<usize>,
    pub background_share: Option<f32>,
    pub candidate_k: Option<usize>,
    pub render: Option<RenderFormat>,
}

impl ContextRequest {
    pub fn validate(&self) -> std::result::Result<(), String> {
        let text = match (&self.query, &self.messages) {
            (Some(q), None) => q.as_str(),
            (None, Some(msgs)) => {
                let last = msgs.last().ok_or("messages must not be empty")?;
                if last.role != Role::User {
                    return Err("last message must be from the user".into());
                }
                last.content.as_str()
            }
            _ => return Err("exactly one of query or messages is required".into()),
        };
        if text.trim().is_empty() {
            return Err("query must not be empty".into());
        }
        if matches!(self.token_budget, Some(b) if b < 200) {
            return Err("token_budget must be at least 200".into());
        }
        if matches!(self.candidate_k, Some(k) if !(1..=200).contains(&k)) {
            return Err("candidate_k must be between 1 and 200".into());
        }
        if matches!(self.background_share, Some(s) if !(0.0..=1.0).contains(&s)) {
            return Err("background_share must be between 0.0 and 1.0".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ResolvedQuerySource {
    Original,
    Rewritten,
    Fallback,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Passage {
    pub ref_id: String,
    pub document_id: DocumentId,
    pub version_num: u32,
    pub source_uri: String,
    pub snapshot_uri: String,
    pub canonical_uri: Option<String>,
    pub section: Option<String>,
    pub page: Option<u32>,
    pub offset_start: usize,
    pub offset_end: usize,
    pub text: String,
    pub chunk_ids: Vec<ChunkId>,
    pub strategies: Vec<String>,
    pub score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackgroundItem {
    pub ref_id: String,
    pub text: String,
    pub level: u32,
    pub document_id: DocumentId,
    pub covers: Vec<ChunkId>,
    pub score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextUsage {
    pub budget: usize,
    pub used: usize,
    pub passages: usize,
    pub background: usize,
    pub dropped_passages: usize,
    pub counter: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyFailure {
    pub strategy: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetrievalInfo {
    pub queries: Vec<String>,
    pub strategies_ok: Vec<String>,
    pub strategies_failed: Vec<StrategyFailure>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextResponse {
    pub resolved_query: String,
    pub resolved_query_source: ResolvedQuerySource,
    pub passages: Vec<Passage>,
    pub background: Vec<BackgroundItem>,
    pub usage: ContextUsage,
    pub retrieval: RetrievalInfo,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rendered: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CandidateList {
    pub query_index: usize,
    pub strategy: RetrievalStrategy,
    pub chunks: Vec<RetrievedChunk>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Candidates {
    pub queries: Vec<String>,
    pub lists: Vec<CandidateList>,
    pub failed: Vec<(RetrievalStrategy, String)>,
}

pub fn strategy_name(s: &RetrievalStrategy) -> &'static str {
    match s {
        RetrievalStrategy::Vector => "vector",
        RetrievalStrategy::Bm25 => "bm25",
        RetrievalStrategy::ColBert => "colbert",
        RetrievalStrategy::Raptor => "raptor",
        RetrievalStrategy::Graph => "graph",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ContextConfig;

    fn user(s: &str) -> Message {
        Message {
            role: Role::User,
            content: s.into(),
        }
    }
    fn assistant(s: &str) -> Message {
        Message {
            role: Role::Assistant,
            content: s.into(),
        }
    }
    fn req() -> ContextRequest {
        ContextRequest {
            collection_id: "docs".into(),
            query: Some("q".into()),
            messages: None,
            token_budget: None,
            background_share: None,
            candidate_k: None,
            render: None,
        }
    }

    #[test]
    fn validate_accepts_minimal_query() {
        assert!(req().validate().is_ok());
    }

    #[test]
    fn validate_rejects_query_and_messages_both_or_neither() {
        let mut r = req();
        r.messages = Some(vec![user("hi")]);
        assert_eq!(
            r.validate().unwrap_err(),
            "exactly one of query or messages is required"
        );
        let mut r = req();
        r.query = None;
        assert_eq!(
            r.validate().unwrap_err(),
            "exactly one of query or messages is required"
        );
    }

    #[test]
    fn validate_rejects_bad_messages() {
        let mut r = req();
        r.query = None;
        r.messages = Some(vec![]);
        assert_eq!(r.validate().unwrap_err(), "messages must not be empty");
        r.messages = Some(vec![user("a"), assistant("b")]);
        assert_eq!(
            r.validate().unwrap_err(),
            "last message must be from the user"
        );
    }

    #[test]
    fn validate_rejects_blank_query_text() {
        let mut r = req();
        r.query = Some("   ".into());
        assert_eq!(r.validate().unwrap_err(), "query must not be empty");
        let mut r = req();
        r.query = None;
        r.messages = Some(vec![user(" \n")]);
        assert_eq!(r.validate().unwrap_err(), "query must not be empty");
    }

    #[test]
    fn validate_rejects_out_of_range_numbers() {
        let mut r = req();
        r.token_budget = Some(199);
        assert_eq!(
            r.validate().unwrap_err(),
            "token_budget must be at least 200"
        );
        let mut r = req();
        r.token_budget = Some(200);
        assert!(r.validate().is_ok());
        for k in [0usize, 201] {
            let mut r = req();
            r.candidate_k = Some(k);
            assert_eq!(
                r.validate().unwrap_err(),
                "candidate_k must be between 1 and 200"
            );
        }
        for s in [-0.1f32, 1.1] {
            let mut r = req();
            r.background_share = Some(s);
            assert_eq!(
                r.validate().unwrap_err(),
                "background_share must be between 0.0 and 1.0"
            );
        }
    }

    #[test]
    fn unknown_role_fails_to_deserialize() {
        let v =
            serde_json::json!({"collection_id":"d","messages":[{"role":"system","content":"x"}]});
        assert!(serde_json::from_value::<ContextRequest>(v).is_err());
    }

    #[test]
    fn response_omits_rendered_when_none_and_uses_lowercase_enums() {
        let resp = ContextResponse {
            resolved_query: "q".into(),
            resolved_query_source: ResolvedQuerySource::Rewritten,
            passages: vec![],
            background: vec![],
            usage: ContextUsage {
                budget: 4000,
                used: 0,
                passages: 0,
                background: 0,
                dropped_passages: 0,
                counter: "cl100k".into(),
            },
            retrieval: RetrievalInfo {
                queries: vec![],
                strategies_ok: vec![],
                strategies_failed: vec![],
            },
            rendered: None,
        };
        let v = serde_json::to_value(&resp).unwrap();
        assert!(v.get("rendered").is_none());
        assert_eq!(v["resolved_query_source"], "rewritten");
    }

    #[test]
    fn strategy_names_are_lowercase() {
        assert_eq!(strategy_name(&RetrievalStrategy::ColBert), "colbert");
        assert_eq!(strategy_name(&RetrievalStrategy::Bm25), "bm25");
    }

    #[test]
    fn context_config_defaults() {
        let c = ContextConfig::default();
        assert_eq!(
            (
                c.default_token_budget,
                c.default_candidate_k,
                c.rewrite_max_messages
            ),
            (4000, 50, 6)
        );
    }
}
