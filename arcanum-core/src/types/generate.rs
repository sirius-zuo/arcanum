use super::context::{ContextRequest, ContextResponse, Message, RenderFormat};
use super::document::{ChunkId, DocumentId};
use crate::traits::{GenerationUsage, StopReason};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GenerateMode {
    #[default]
    Answer,
    Summarize,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GenerateContextOptions {
    pub token_budget: Option<usize>,
    pub background_share: Option<f32>,
    pub candidate_k: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerateRequest {
    pub collection_id: String,
    #[serde(default)]
    pub mode: GenerateMode,
    pub query: Option<String>,
    pub messages: Option<Vec<Message>>,
    pub generator: Option<String>,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
    pub instructions: Option<String>,
    #[serde(default)]
    pub context: GenerateContextOptions,
    #[serde(default)]
    pub stream: bool,
}

impl GenerateRequest {
    /// The request's own `token_budget` wins over `default_token_budget`.
    pub fn to_context_request(&self, default_token_budget: Option<usize>) -> ContextRequest {
        ContextRequest {
            collection_id: self.collection_id.clone(),
            query: self.query.clone(),
            messages: self.messages.clone(),
            token_budget: self.context.token_budget.or(default_token_budget),
            background_share: self.context.background_share,
            candidate_k: self.context.candidate_k,
            render: Some(RenderFormat::Xml),
        }
    }

    pub fn validate(&self) -> std::result::Result<(), String> {
        self.to_context_request(None).validate()?;
        if self.max_tokens == Some(0) {
            return Err("max_tokens must be at least 1".into());
        }
        if matches!(self.temperature, Some(t) if !(0.0..=2.0).contains(&t)) {
            return Err("temperature must be between 0.0 and 2.0".into());
        }
        if matches!(&self.instructions, Some(i) if i.chars().count() > 2000) {
            return Err("instructions must be at most 2000 characters".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GenerateStatus {
    Ok,
    NoContext,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Citation {
    pub ref_id: String,
    pub chunk_ids: Vec<ChunkId>,
    pub document_id: DocumentId,
    pub version_num: u32,
    pub source_uri: String,
    pub offset_start: usize,
    pub offset_end: usize,
    pub answer_spans: Vec<(usize, usize)>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GeneratorInfo {
    pub name: String,
    pub model: String,
}

/// The SSE `done` payload; flattened into `GenerateResponse` for JSON.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GenerateOutcome {
    pub status: GenerateStatus,
    pub citations: Vec<Citation>,
    pub unknown_refs: Vec<String>,
    pub stop_reason: StopReason,
    pub usage: GenerationUsage,
    pub generator: GeneratorInfo,
}

#[derive(Debug, Clone, Serialize)]
pub struct GenerateResponse {
    pub answer: String,
    #[serde(flatten)]
    pub outcome: GenerateOutcome,
    pub context: ContextResponse,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{ContextUsage, ResolvedQuerySource, RetrievalInfo};
    use serde_json::json;

    fn base() -> GenerateRequest {
        GenerateRequest {
            collection_id: "d".into(),
            mode: GenerateMode::Answer,
            query: Some("q".into()),
            messages: None,
            generator: None,
            max_tokens: None,
            temperature: None,
            instructions: None,
            context: Default::default(),
            stream: false,
        }
    }

    #[test]
    fn validate_rejects_generate_specific_fields() {
        assert!(base().validate().is_ok());
        let mut r = base();
        r.max_tokens = Some(0);
        assert_eq!(r.validate().unwrap_err(), "max_tokens must be at least 1");
        for t in [-0.1f32, 2.1] {
            let mut r = base();
            r.temperature = Some(t);
            assert_eq!(
                r.validate().unwrap_err(),
                "temperature must be between 0.0 and 2.0"
            );
        }
        let mut r = base();
        r.instructions = Some("é".repeat(2001));
        assert_eq!(
            r.validate().unwrap_err(),
            "instructions must be at most 2000 characters"
        );
        let mut r = base();
        r.instructions = Some("é".repeat(2000));
        assert!(r.validate().is_ok());
        let mut r = base();
        r.context.token_budget = Some(100);
        assert_eq!(
            r.validate().unwrap_err(),
            "token_budget must be at least 200"
        );
    }

    #[test]
    fn mode_defaults_to_answer_and_unknown_mode_fails() {
        let r: GenerateRequest =
            serde_json::from_value(json!({"collection_id":"d","query":"q"})).unwrap();
        assert_eq!(r.mode, GenerateMode::Answer);
        assert!(!r.stream);
        assert!(serde_json::from_value::<GenerateRequest>(
            json!({"collection_id":"d","query":"q","mode":"chat"})
        )
        .is_err());
    }

    #[test]
    fn to_context_request_forces_xml_and_prefers_request_budget() {
        let c = base().to_context_request(Some(8000));
        assert_eq!(c.token_budget, Some(8000));
        assert_eq!(c.render, Some(RenderFormat::Xml));
        assert_eq!(c.query.as_deref(), Some("q"));
        let mut r = base();
        r.context.token_budget = Some(500);
        assert_eq!(r.to_context_request(Some(8000)).token_budget, Some(500));
        assert_eq!(base().to_context_request(None).token_budget, None);
    }

    #[test]
    fn response_flattens_outcome_and_serializes_nulls() {
        let context = ContextResponse {
            resolved_query: "q".into(),
            resolved_query_source: ResolvedQuerySource::Original,
            passages: vec![],
            background: vec![],
            usage: ContextUsage {
                budget: 0,
                used: 0,
                passages: 0,
                background: 0,
                dropped_passages: 0,
                counter: "c".into(),
            },
            retrieval: RetrievalInfo {
                queries: vec![],
                strategies_ok: vec![],
                strategies_failed: vec![],
            },
            rendered: None,
        };
        let resp = GenerateResponse {
            answer: "x [P1]".into(),
            outcome: GenerateOutcome {
                status: GenerateStatus::NoContext,
                citations: vec![],
                unknown_refs: vec![],
                stop_reason: StopReason::Other("content_filter".into()),
                usage: GenerationUsage::default(),
                generator: GeneratorInfo {
                    name: "smart".into(),
                    model: "m".into(),
                },
            },
            context,
        };
        let mut v = serde_json::to_value(&resp).unwrap();
        assert_eq!(v["status"], "no_context");
        assert_eq!(v["stop_reason"], "other");
        assert_eq!(
            v["usage"],
            json!({"input_tokens":null,"output_tokens":null})
        );
        assert!(v["context"].is_object());
        assert_eq!(v["generator"]["name"], "smart");
        let c = Citation {
            ref_id: "P1".into(),
            chunk_ids: vec![],
            document_id: DocumentId::default(),
            version_num: 1,
            source_uri: "u".into(),
            offset_start: 0,
            offset_end: 1,
            answer_spans: vec![(25, 29)],
        };
        v = serde_json::to_value(&c).unwrap();
        assert_eq!(v["answer_spans"], json!([[25, 29]]));
    }
}
