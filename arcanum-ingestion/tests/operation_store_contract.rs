//! Shared contract suite for `OperationStore` adapters (PostgreSQL + SQLite).
//!
//! The suite is parameterized over a store factory and exercises every
//! requirement from the durable-operations plan's Task 2:
//!   - create returns `is_new = true` with status `Accepted`
//!   - duplicate idempotency returns the SAME operation id (`is_new = false`)
//!   - conflicting reuse of an idempotency key fails (typed conflict)
//!   - `Accepted -> Running -> Succeeded`
//!   - `Accepted -> Failed`
//!   - terminal-state immutability (identical report reapply idempotent,
//!     a different report or any other transition conflicts)
//!   - successful content-URI persistence
//!   - get-by-ID and get-by-idempotency-key
//!   - restart recovery: a fresh store over the same storage sees the
//!     operation and its terminal report

use arcanum_core::traits::OperationStore;
use arcanum_core::types::{
    CollectionId, IngestionOutcome, IngestionReport, IngestionSubmission, OperationStatus,
    PartialOutputDisposition, SafeOperationError,
};
use arcanum_core::{ArcanumError, Result};
use arcanum_ingestion::operations::postgres::PostgresOperationStore;
use arcanum_ingestion::operations::sqlite::SqliteOperationStore;
use chrono::Utc;

/// Local PostgreSQL used for the contract suite. Override with
/// `TEST_DATABASE_URL` to target a different instance. The `arcanum_test`
/// database must exist (see the project migrations / task report).
const DEFAULT_TEST_DATABASE_URL: &str =
    "postgres://yeehai:yeehai_local_test_only@localhost:5432/arcanum_test";

fn is_conflict(e: &ArcanumError) -> bool {
    matches!(e, ArcanumError::Conflict(_))
}

fn submission(key: &str, source_uri: &str, collection: &str) -> IngestionSubmission {
    IngestionSubmission {
        idempotency_key: key.to_string(),
        logical_source_uri: source_uri.to_string(),
        mime_hint: Some("application/pdf".to_string()),
        collection_id: CollectionId(collection.to_string()),
        pipeline_configuration: serde_json::json!({ "template": "standard" }),
        payload: None,
        payload_locator: None,
    }
}

fn succeeded_report(operation_id: &arcanum_core::types::OperationId) -> IngestionReport {
    IngestionReport {
        operation_id: operation_id.clone(),
        status: OperationStatus::Succeeded,
        outcome: Some(IngestionOutcome::Ingested),
        content_uri: Some("s3://arcanum/raw/document-a/version-1".to_string()),
        error: None,
        partial_output_disposition: None,
    }
}

fn failed_report(operation_id: &arcanum_core::types::OperationId) -> IngestionReport {
    IngestionReport {
        operation_id: operation_id.clone(),
        status: OperationStatus::Failed,
        outcome: None,
        content_uri: None,
        error: Some(SafeOperationError {
            code: "QUEUE_REJECTED".to_string(),
            message: "queue rejected".to_string(),
            retryable: true,
        }),
        partial_output_disposition: Some(PartialOutputDisposition::Discarded),
    }
}

async fn run_store_contract<S, F, Fut>(new_store: F)
where
    S: OperationStore,
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = S>,
{
    let store = new_store().await;
    let key = format!("idem-{}", uuid::Uuid::new_v4());
    let sub = submission(&key, "https://example.com/document-a", "col-a");

    // 1. create returns is_new = true, status Accepted.
    let created = store.create_or_get(&sub).await.expect("create_or_get");
    assert!(created.is_new, "first create must be new");
    assert_eq!(created.operation.status, OperationStatus::Accepted);
    assert_eq!(created.operation.submission.idempotency_key, key);
    let op_id = created.operation.operation_id.clone();

    // 2. duplicate idempotency returns the SAME id, not new.
    let dup = store
        .create_or_get(&sub)
        .await
        .expect("duplicate create_or_get");
    assert!(!dup.is_new, "duplicate must not be new");
    assert_eq!(
        dup.operation.operation_id, op_id,
        "duplicate must return same id"
    );

    // 3. conflicting reuse of the idempotency key fails.
    let conflict_sub = submission(&key, "https://example.com/DIFFERENT-document", "col-a");
    let err = store
        .create_or_get(&conflict_sub)
        .await
        .expect_err("conflicting reuse must fail");
    assert!(is_conflict(&err), "expected conflict, got {err}");

    // 4. Accepted -> Running -> Succeeded, with content-URI persistence.
    store
        .mark_running(&op_id, Utc::now())
        .await
        .expect("mark_running");
    let running = store
        .get(&op_id)
        .await
        .expect("get running")
        .expect("exists");
    assert_eq!(running.status, OperationStatus::Running);

    let report = succeeded_report(&op_id);
    store.complete(&report).await.expect("complete succeeded");
    let succeeded = store
        .get(&op_id)
        .await
        .expect("get succeeded")
        .expect("exists");
    assert_eq!(succeeded.status, OperationStatus::Succeeded);
    let stored_report = succeeded.terminal_report.as_ref().expect("terminal report");
    assert_eq!(
        stored_report.content_uri.as_deref(),
        Some("s3://arcanum/raw/document-a/version-1"),
        "successful report must persist the content URI"
    );

    // 5. terminal-state immutability.
    //    Reapplying the IDENTICAL terminal report is idempotent.
    store
        .complete(&report)
        .await
        .expect("reapply identical report");
    //    A DIFFERENT terminal report conflicts.
    let mut diff = report.clone();
    diff.content_uri = Some("s3://arcanum/raw/document-a/version-2".to_string());
    let err = store
        .complete(&diff)
        .await
        .expect_err("different terminal report must conflict");
    assert!(is_conflict(&err), "expected conflict, got {err}");
    //    Any other transition from a terminal state conflicts (Succeeded -> Failed).
    let fail_after_success = failed_report(&op_id);
    let err = store
        .complete(&fail_after_success)
        .await
        .expect_err("Succeeded -> Failed must conflict");
    assert!(is_conflict(&err), "expected conflict, got {err}");

    // 6. Accepted -> Failed (e.g. queue rejected before a worker picks it up).
    let key2 = format!("idem-fail-{}", uuid::Uuid::new_v4());
    let sub2 = submission(&key2, "https://example.com/document-b", "col-b");
    let created2 = store.create_or_get(&sub2).await.expect("create_or_get #2");
    let op2_id = created2.operation.operation_id.clone();
    let fail_report = failed_report(&op2_id);
    store
        .complete(&fail_report)
        .await
        .expect("complete failed from accepted");
    let failed = store
        .get(&op2_id)
        .await
        .expect("get failed")
        .expect("exists");
    assert_eq!(failed.status, OperationStatus::Failed);
    assert_eq!(
        failed
            .terminal_report
            .as_ref()
            .unwrap()
            .error
            .as_ref()
            .unwrap()
            .code,
        "QUEUE_REJECTED"
    );

    // 7. get-by-idempotency-key returns the same operation.
    let by_key = store
        .get_by_idempotency(&key)
        .await
        .expect("get_by_idempotency")
        .expect("exists");
    assert_eq!(by_key.operation_id, op_id);

    // 8. restart recovery: a fresh store over the same storage sees the
    //    operation and its terminal report.
    let store2 = new_store().await;
    let recovered = store2
        .get(&op_id)
        .await
        .expect("recovered get")
        .expect("exists");
    assert_eq!(recovered.status, OperationStatus::Succeeded);
    assert_eq!(
        recovered
            .terminal_report
            .as_ref()
            .unwrap()
            .content_uri
            .as_deref(),
        Some("s3://arcanum/raw/document-a/version-1")
    );
    let recovered_by_key = store2
        .get_by_idempotency(&key)
        .await
        .expect("recovered by key")
        .expect("exists");
    assert_eq!(recovered_by_key.operation_id, op_id);
    let recovered_failed = store2
        .get(&op2_id)
        .await
        .expect("recovered failed get")
        .expect("exists");
    assert_eq!(recovered_failed.status, OperationStatus::Failed);
    assert_eq!(
        recovered_failed
            .terminal_report
            .as_ref()
            .unwrap()
            .error
            .as_ref()
            .unwrap()
            .code,
        "QUEUE_REJECTED"
    );

    let _: Result<()> = Ok(());
}

#[tokio::test]
async fn sqlite_store_contract() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("operations.db");
    let path_str = path.to_str().expect("utf8 path").to_string();
    run_store_contract(|| async {
        SqliteOperationStore::open(&path_str)
            .await
            .expect("open sqlite")
    })
    .await;
}

#[tokio::test]
async fn postgres_store_contract() {
    let url = std::env::var("TEST_DATABASE_URL")
        .unwrap_or_else(|_| DEFAULT_TEST_DATABASE_URL.to_string());
    run_store_contract(|| async {
        PostgresOperationStore::new(&url)
            .await
            .expect("connect postgres")
    })
    .await;
}
