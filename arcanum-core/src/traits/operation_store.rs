use async_trait::async_trait;
use crate::Result;
use crate::types::{
    CreateOperationResult, IngestionOperation, IngestionReport, IngestionSubmission, OperationId,
};
use chrono::{DateTime, Utc};

/// Durable store for ingestion operation lifecycle state.
///
/// Terminal truth must survive process restart and WebSocket loss: every
/// state transition and the terminal report are persisted here, and event
/// emission is only a latency optimization.
#[async_trait]
pub trait OperationStore: Send + Sync {
    /// Create a new operation from a submission, or return the existing
    /// operation for the same idempotency key. `CreateOperationResult::is_new`
    /// distinguishes the two so callers can avoid enqueueing duplicate work.
    async fn create_or_get(&self, submission: &IngestionSubmission) -> Result<CreateOperationResult>;

    /// Transition `Accepted -> Running`.
    async fn mark_running(&self, id: &OperationId, started_at: DateTime<Utc>) -> Result<()>;

    /// Persist a terminal report (`Succeeded` or `Failed`). The report status
    /// must be a valid transition from the operation's current state.
    async fn complete(&self, report: &IngestionReport) -> Result<()>;

    /// Fetch an operation by its stable identifier.
    async fn get(&self, id: &OperationId) -> Result<Option<IngestionOperation>>;

    /// Fetch an operation by its idempotency key, so idempotent replay returns
    /// the original operation and its terminal report.
    async fn get_by_idempotency(&self, key: &str) -> Result<Option<IngestionOperation>>;
}
