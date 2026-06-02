use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use super::document::CollectionId;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationId(pub Uuid);

impl OperationId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

#[derive(Debug, Clone)]
pub struct IngestionTask {
    pub operation_id:      OperationId,
    pub source_uri:        String,
    pub collection_id:     CollectionId,
    pub pipeline_template: String,
    pub attempt:           u32,
    pub force:             bool,
    /// Inline content for direct uploads. When present, the worker builds
    /// `Source::Raw` from these bytes instead of resolving `source_uri`.
    pub content:           Option<Vec<u8>>,
    /// MIME hint for inline content (derived from the upload filename).
    pub mime_hint:         Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OperationStatus {
    Pending,
    Processing,
    PartialSuccess,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngestionReport {
    pub operation_id: OperationId,
    pub status: OperationStatus,
    pub stages_completed: Vec<String>,
    pub stages_failed: Vec<(String, String)>,
    pub chunks_indexed: usize,
    pub duration_ms: u64,
    pub completed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Citation {
    pub document_uri: String,
    pub document_title: Option<String>,
    pub section: Option<String>,
    pub chunk_index: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetrievalResult {
    pub chunks: Vec<super::document::RetrievedChunk>,
    pub citations: Vec<Citation>,
    pub strategy_scores: HashMap<String, f32>,
    pub confidence: f32,
}
