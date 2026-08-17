use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use super::document::CollectionId;
use crate::ArcanumError;

/// Stable identifier for a durable ingestion operation. This is what Core
/// receives on submission and later queries by.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
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
    pub force:             bool,
    /// Inline content for direct uploads. When present, the worker builds
    /// `Source::Raw` from these bytes instead of resolving `source_uri`.
    pub content:           Option<Vec<u8>>,
    /// MIME hint for inline content (derived from the upload filename).
    pub mime_hint:         Option<String>,
    /// Durable locator for a payload staged in the `OperationPayloadStore`.
    /// Set when `content` is `None` and the submission was staged before
    /// `create_or_get`; the worker resolves `Source::Raw` from these bytes.
    pub payload_locator:   Option<String>,
}

/// Lifecycle of a durable ingestion operation.
///
/// Accepted or queued is not success. Terminal truth is a `Succeeded` or
/// `Failed` report persisted to the `OperationStore`, never a WebSocket event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum OperationStatus {
    /// Submitted and durably persisted; not yet picked up by a worker.
    Accepted,
    /// A worker has started processing this operation.
    Running,
    /// Terminal success — the report MUST carry `content_uri`.
    Succeeded,
    /// Terminal failure — the report MAY carry a safe error and a partial-output disposition.
    Failed,
}

/// What a terminal `Succeeded` report means for the original content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum IngestionOutcome {
    /// New or changed content was ingested.
    Ingested,
    /// Deduplication determined the content is unchanged; the existing
    /// version-specific original-content URI is returned.
    Unchanged,
}

/// What was done with partially-written outputs when an operation failed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PartialOutputDisposition {
    /// Partial outputs were discarded and are not queryable.
    Discarded,
    /// Partial outputs were isolated behind a marker so consumers can find them.
    Isolated,
}

/// A safe, redacted error for a durable operation. Carries no internal stack
/// or connection details — only a stable code, a human message, and whether a
/// retry could succeed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SafeOperationError {
    pub code:      String,
    pub message:   String,
    pub retryable: bool,
}

/// The canonical terminal report for a durable ingestion operation.
///
/// Global constraint: a successful or unchanged-content report MUST contain
/// the applicable version-specific original-content URI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IngestionReport {
    pub operation_id:                OperationId,
    pub status:                      OperationStatus,
    pub outcome:                     Option<IngestionOutcome>,
    pub content_uri:                 Option<String>,
    pub error:                       Option<SafeOperationError>,
    /// How partial outputs were handled when `status` is `Failed`.
    pub partial_output_disposition:  Option<PartialOutputDisposition>,
}

impl IngestionReport {
    /// Terminal success for new or changed content. The report MUST carry the
    /// version-specific original-content URI produced by the pipeline.
    pub fn succeeded(operation_id: OperationId, content_uri: String) -> Self {
        Self {
            operation_id,
            status: OperationStatus::Succeeded,
            outcome: Some(IngestionOutcome::Ingested),
            content_uri: Some(content_uri),
            error: None,
            partial_output_disposition: None,
        }
    }

    /// Terminal success for deduplicated (unchanged) content. `content_uri` is
    /// the EXISTING version's snapshot URI, not a fresh one.
    pub fn unchanged(operation_id: OperationId, content_uri: String) -> Self {
        Self {
            operation_id,
            status: OperationStatus::Succeeded,
            outcome: Some(IngestionOutcome::Unchanged),
            content_uri: Some(content_uri),
            error: None,
            partial_output_disposition: None,
        }
    }

    /// Terminal failure with a safe, redacted error and no partial-output
    /// disposition recorded (nothing was written yet, or the caller does not
    /// track it).
    pub fn failed(operation_id: OperationId, code: &str, message: String, retryable: bool) -> Self {
        Self::failed_with_disposition(operation_id, code, message, retryable, None)
    }

    /// Terminal failure that also records how partially-written outputs were
    /// handled when the operation failed mid-pipeline.
    pub fn failed_with_disposition(
        operation_id: OperationId,
        code: &str,
        message: String,
        retryable: bool,
        disposition: Option<PartialOutputDisposition>,
    ) -> Self {
        Self {
            operation_id,
            status: OperationStatus::Failed,
            outcome: None,
            content_uri: None,
            error: Some(SafeOperationError {
                code: code.to_string(),
                message,
                retryable,
            }),
            partial_output_disposition: disposition,
        }
    }

    /// Terminal failure recorded when a submission was durably persisted but
    /// the queue refused the task (e.g. the queue is full). The operation is
    /// `Failed` with code `QUEUE_REJECTED`; a full queue is transient and thus
    /// retryable.
    pub fn queue_rejected(operation: IngestionOperation, err: &ArcanumError) -> Self {
        Self::failed(
            operation.operation_id,
            "QUEUE_REJECTED",
            format!("work was not enqueued: {err}"),
            matches!(err, ArcanumError::QueueFull),
        )
    }
}

/// What Core submits to start a durable ingestion operation. The logical
/// source URI is stable and is never replaced by a filename, temporary URL, or
/// staging key.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IngestionSubmission {
    pub idempotency_key: String,
    pub logical_source_uri: String,
    pub mime_hint: Option<String>,
    pub collection_id: CollectionId,
    pub pipeline_configuration: serde_json::Value,
    /// Inline payload bytes, XOR `payload_locator`. Only one is set.
    pub payload: Option<Vec<u8>>,
    /// Durable locator for a payload already staged by the `OperationPayloadStore`.
    pub payload_locator: Option<String>,
}

/// The canonical, queryable document for a durable ingestion operation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IngestionOperation {
    pub operation_id: OperationId,
    pub submission: IngestionSubmission,
    pub status: OperationStatus,
    pub accepted_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub terminal_report: Option<IngestionReport>,
}

/// Result of `OperationStore::create_or_get`.
#[derive(Debug, Clone, PartialEq)]
pub struct CreateOperationResult {
    pub operation: IngestionOperation,
    pub is_new: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Citation {
    pub document_uri: String,
    pub document_title: Option<String>,
    pub section: Option<String>,
    pub chunk_index: usize,
    pub version: Option<u32>,
    pub snapshot_uri: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetrievalResult {
    pub chunks: Vec<super::document::RetrievedChunk>,
    pub citations: Vec<Citation>,
    pub strategy_scores: HashMap<String, f32>,
    pub confidence: f32,
}
