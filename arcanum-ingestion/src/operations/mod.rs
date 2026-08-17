//! Durable ingestion operation stores and payload adapters.
//!
//! Task 2 of the durable-operations plan: PostgreSQL + SQLite `OperationStore`
//! adapters with identical semantics, plus local and S3-compatible
//! `OperationPayloadStore` adapters.

pub mod payload;
pub mod postgres;
pub mod sqlite;

pub use payload::local::LocalOperationPayloadStore;
pub use payload::s3::{LocalFsObjectStore, S3ObjectStore, S3OperationPayloadStore};
pub use postgres::PostgresOperationStore;
pub use sqlite::SqliteOperationStore;

use arcanum_core::types::{
    IngestionOperation, IngestionReport, IngestionSubmission, OperationStatus,
};
use arcanum_core::{ArcanumError, Result};
use sha2::Digest;

/// Stable hash of a submission, used to detect conflicting reuse of an
/// idempotency key. Hashing the full serialized submission means an identical
/// replay hashes identically, while any change to the submission — the logical
/// source URI, collection, pipeline configuration, or payload — yields a
/// different hash and a conflict.
pub(crate) fn submission_hash(submission: &IngestionSubmission) -> Result<String> {
    let json = serde_json::to_vec(submission)
        .map_err(|e| ArcanumError::Storage(format!("serialize submission for hashing: {e}")))?;
    let digest = sha2::Sha256::digest(&json);
    Ok(hex::encode(digest))
}

pub(crate) fn parse_status(s: &str) -> Result<OperationStatus> {
    match s {
        "accepted" => Ok(OperationStatus::Accepted),
        "running" => Ok(OperationStatus::Running),
        "succeeded" => Ok(OperationStatus::Succeeded),
        "failed" => Ok(OperationStatus::Failed),
        other => Err(ArcanumError::Storage(format!(
            "unknown operation status: {other}"
        ))),
    }
}

pub(crate) fn status_str(s: &OperationStatus) -> &'static str {
    match s {
        OperationStatus::Accepted => "accepted",
        OperationStatus::Running => "running",
        OperationStatus::Succeeded => "succeeded",
        OperationStatus::Failed => "failed",
    }
}

/// Validates a terminal transition per the plan:
/// - `Accepted -> Failed` (e.g. queue rejected before a worker picks it up) is
///   valid; `Accepted -> Running` is handled by `mark_running`.
/// - `Running -> Succeeded | Failed` are valid, but a `Running -> Succeeded`
///   report MUST carry the original-content URI (`content_uri`): a successful
///   report is terminal truth for where the content lives, and persisting one
///   without it would make `None` terminal truth.
/// - Re-applying the IDENTICAL terminal report on a terminal operation is
///   idempotent.
/// - Any other transition — including a different terminal report on a terminal
///   operation — is a typed conflict.
pub(crate) fn validate_transition(
    current: &IngestionOperation,
    report: &IngestionReport,
) -> Result<()> {
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
