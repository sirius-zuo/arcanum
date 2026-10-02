use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sha2::Digest;
use std::collections::HashMap;
use std::sync::Mutex;

use super::OperationStore;
use crate::types::{
    CreateOperationResult, IngestionOperation, IngestionReport, IngestionSubmission, OperationId,
    OperationStatus,
};
use crate::{ArcanumError, Result};

/// In-memory `OperationStore` for development and tests. State does not survive
/// process restart; production and durable local configurations use
/// `PostgresOperationStore` / `SqliteOperationStore`.
pub struct InMemoryOperationStore {
    data: Mutex<Inner>,
}

struct Inner {
    by_id: HashMap<OperationId, StoredOperation>,
    by_key: HashMap<String, OperationId>,
}

struct StoredOperation {
    submission_hash: String,
    operation: IngestionOperation,
}

impl InMemoryOperationStore {
    pub fn new() -> Self {
        Self {
            data: Mutex::new(Inner {
                by_id: HashMap::new(),
                by_key: HashMap::new(),
            }),
        }
    }
}

impl Default for InMemoryOperationStore {
    fn default() -> Self {
        Self::new()
    }
}

fn submission_hash(submission: &IngestionSubmission) -> Result<String> {
    let json = serde_json::to_vec(submission)
        .map_err(|e| ArcanumError::Storage(format!("serialize submission for hashing: {e}")))?;
    let digest = sha2::Sha256::digest(&json);
    Ok(hex::encode(digest))
}

/// Reconstruct the operation as a durable store would serve it over a read
/// path: `submission.payload` is never returned (payload bytes are only kept
/// in the DB for idempotency hashing, never surfaced). The full submission
/// stays stored internally so `create_or_get` conflict hashing still works.
fn as_queried(stored: &StoredOperation) -> IngestionOperation {
    let mut operation = stored.operation.clone();
    operation.submission.payload = None;
    operation
}

/// Mirrors the transition guards of the durable adapters:
/// `Accepted -> Running | Failed`, `Running -> Succeeded` (with content URI) |
/// `Failed`, and idempotent reapplication of an IDENTICAL terminal report.
fn validate_transition(current: &IngestionOperation, report: &IngestionReport) -> Result<()> {
    let next = &report.status;
    match (&current.status, next) {
        (OperationStatus::Accepted, OperationStatus::Running | OperationStatus::Failed) => Ok(()),
        (OperationStatus::Running, OperationStatus::Succeeded) if report.content_uri.is_some() => {
            Ok(())
        }
        (OperationStatus::Running, OperationStatus::Succeeded) => Err(ArcanumError::Conflict(
            "succeeded report must carry content_uri".to_string(),
        )),
        (OperationStatus::Running, OperationStatus::Failed) => Ok(()),
        (OperationStatus::Succeeded | OperationStatus::Failed, st)
            if st == &current.status && current.terminal_report.as_ref() == Some(report) =>
        {
            Ok(())
        }
        _ => Err(ArcanumError::Conflict(
            "invalid operation transition".to_string(),
        )),
    }
}

#[async_trait]
impl OperationStore for InMemoryOperationStore {
    async fn create_or_get(
        &self,
        submission: &IngestionSubmission,
    ) -> Result<CreateOperationResult> {
        let hash = submission_hash(submission)?;
        let mut data = self.data.lock().unwrap();
        if let Some(op_id) = data.by_key.get(&submission.idempotency_key) {
            let stored = data
                .by_id
                .get(op_id)
                .ok_or_else(|| ArcanumError::Storage("idempotency key row vanished".into()))?;
            if stored.submission_hash == hash {
                return Ok(CreateOperationResult {
                    operation: as_queried(stored),
                    is_new: false,
                });
            }
            return Err(ArcanumError::Conflict(format!(
                "idempotency key {} already used by a different submission",
                submission.idempotency_key
            )));
        }
        let operation = IngestionOperation {
            operation_id: OperationId::new(),
            submission: submission.clone(),
            status: OperationStatus::Accepted,
            accepted_at: Utc::now(),
            started_at: None,
            terminal_report: None,
        };
        data.by_id.insert(
            operation.operation_id.clone(),
            StoredOperation {
                submission_hash: hash,
                operation: operation.clone(),
            },
        );
        data.by_key.insert(
            submission.idempotency_key.clone(),
            operation.operation_id.clone(),
        );
        Ok(CreateOperationResult {
            operation,
            is_new: true,
        })
    }

    async fn mark_running(&self, id: &OperationId, started_at: DateTime<Utc>) -> Result<()> {
        let mut data = self.data.lock().unwrap();
        let stored = data
            .by_id
            .get_mut(id)
            .ok_or_else(|| ArcanumError::NotFound(format!("operation not found: {}", id.0)))?;
        if stored.operation.status != OperationStatus::Accepted {
            return Err(ArcanumError::Conflict(format!(
                "cannot mark running: operation {} is {:?}",
                id.0, stored.operation.status
            )));
        }
        stored.operation.status = OperationStatus::Running;
        stored.operation.started_at = Some(started_at);
        Ok(())
    }

    async fn complete(&self, report: &IngestionReport) -> Result<()> {
        let mut data = self.data.lock().unwrap();
        let stored = data.by_id.get_mut(&report.operation_id).ok_or_else(|| {
            ArcanumError::NotFound(format!("operation not found: {}", report.operation_id.0))
        })?;
        validate_transition(&stored.operation, report)?;
        stored.operation.status = report.status.clone();
        stored.operation.terminal_report = Some(report.clone());
        Ok(())
    }

    async fn get(&self, id: &OperationId) -> Result<Option<IngestionOperation>> {
        let data = self.data.lock().unwrap();
        Ok(data.by_id.get(id).map(as_queried))
    }

    async fn get_by_idempotency(&self, key: &str) -> Result<Option<IngestionOperation>> {
        let data = self.data.lock().unwrap();
        Ok(data
            .by_key
            .get(key)
            .and_then(|id| data.by_id.get(id))
            .map(as_queried))
    }
}
