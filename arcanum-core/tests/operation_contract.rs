//! Contract tests for the canonical durable ingestion operation types.
//!
//! These define the serialization contract between Core (the Knowledge Nexus
//! caller) and Arcanum. Terminal reports must carry the version-specific
//! original-content URI on success, and the safe, redacted error shape on
//! failure.

use arcanum_core::types::{
    CollectionId, IngestionOperation, IngestionOutcome, IngestionReport, IngestionSubmission,
    OperationId, OperationStatus, PartialOutputDisposition, SafeOperationError,
};
use chrono::{DateTime, Utc};

fn succeeded_report() -> IngestionReport {
    IngestionReport {
        operation_id: OperationId::new(),
        status: OperationStatus::Succeeded,
        outcome: Some(IngestionOutcome::Ingested),
        content_uri: Some("s3://arcanum/raw/document-a/version-1".to_string()),
        error: None,
        partial_output_disposition: None,
    }
}

fn fixed_timestamp() -> DateTime<Utc> {
    DateTime::parse_from_rfc3339("2026-08-10T12:00:00Z")
        .unwrap()
        .with_timezone(&Utc)
}

#[test]
fn terminal_report_round_trips_original_content_uri() {
    let report = succeeded_report();
    let json = serde_json::to_value(&report).unwrap();
    assert_eq!(json["content_uri"], "s3://arcanum/raw/document-a/version-1");
    assert_eq!(serde_json::from_value::<IngestionReport>(json).unwrap(), report);
}

#[test]
fn operation_status_round_trips_all_variants() {
    let statuses = [
        OperationStatus::Accepted,
        OperationStatus::Running,
        OperationStatus::Succeeded,
        OperationStatus::Failed,
    ];
    for status in statuses {
        let json = serde_json::to_value(&status).unwrap();
        let back: OperationStatus = serde_json::from_value(json).unwrap();
        assert_eq!(back, status);
    }
}

#[test]
fn ingestion_outcome_round_trips_all_variants() {
    let outcomes = [IngestionOutcome::Ingested, IngestionOutcome::Unchanged];
    for outcome in outcomes {
        let json = serde_json::to_value(&outcome).unwrap();
        let back: IngestionOutcome = serde_json::from_value(json).unwrap();
        assert_eq!(back, outcome);
    }
}

#[test]
fn ingestion_submission_round_trips() {
    let submission = IngestionSubmission {
        idempotency_key: "idem-1".to_string(),
        logical_source_uri: "https://example.com/document-a".to_string(),
        mime_hint: Some("application/pdf".to_string()),
        collection_id: CollectionId("col-1".to_string()),
        pipeline_configuration: serde_json::json!({ "template": "standard" }),
        payload: Some(vec![1, 2, 3]),
        payload_locator: None,
    };
    let json = serde_json::to_value(&submission).unwrap();
    let back: IngestionSubmission = serde_json::from_value(json).unwrap();
    assert_eq!(back, submission);
}

#[test]
fn ingestion_operation_round_trips() {
    let operation = IngestionOperation {
        operation_id: OperationId::new(),
        submission: IngestionSubmission {
            idempotency_key: "idem-2".to_string(),
            logical_source_uri: "https://example.com/document-b".to_string(),
            mime_hint: None,
            collection_id: CollectionId("col-2".to_string()),
            pipeline_configuration: serde_json::json!({ "template": "standard" }),
            payload: None,
            payload_locator: Some("local://operations/idem-2".to_string()),
        },
        status: OperationStatus::Succeeded,
        accepted_at: fixed_timestamp(),
        started_at: Some(fixed_timestamp()),
        terminal_report: Some(succeeded_report()),
    };
    let json = serde_json::to_value(&operation).unwrap();
    let back: IngestionOperation = serde_json::from_value(json).unwrap();
    assert_eq!(back, operation);
}

#[test]
fn failed_report_round_trips_safe_error_and_partial_output_disposition() {
    let report = IngestionReport {
        operation_id: OperationId::new(),
        status: OperationStatus::Failed,
        outcome: None,
        content_uri: None,
        error: Some(SafeOperationError {
            code: "QUEUE_REJECTED".to_string(),
            message: "queue rejected".to_string(),
            retryable: true,
        }),
        partial_output_disposition: Some(PartialOutputDisposition::Discarded),
    };
    let json = serde_json::to_value(&report).unwrap();
    let back: IngestionReport = serde_json::from_value(json).unwrap();
    assert_eq!(back, report);
}

#[test]
fn unchanged_content_report_preserves_content_uri() {
    let report = IngestionReport {
        operation_id: OperationId::new(),
        status: OperationStatus::Succeeded,
        outcome: Some(IngestionOutcome::Unchanged),
        content_uri: Some("s3://arcanum/raw/document-a/version-1".to_string()),
        error: None,
        partial_output_disposition: None,
    };
    let json = serde_json::to_value(&report).unwrap();
    assert_eq!(json["content_uri"], "s3://arcanum/raw/document-a/version-1");
    assert_eq!(serde_json::from_value::<IngestionReport>(json).unwrap(), report);
}
