use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnrichRequest {
    pub text: String,
    pub intent: EnrichIntent,
    pub context: Option<EnrichContext>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EnrichIntent {
    ContextPrefix,
    Summarize,
    ExtractEntities,
    Caption,
    Rerank,
    Custom(String),
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EnrichContext {
    pub document_title: Option<String>,
    pub section: Option<String>,
    pub adjacent_chunks: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnrichedText(pub String);
