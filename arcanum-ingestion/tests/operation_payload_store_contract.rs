//! Shared contract suite for `OperationPayloadStore` adapters (local + S3).
//!
//! The suite is parameterized over a payload-store factory and exercises the
//! plan's Task 2 Step 5 requirements:
//!   - staged bytes survive adapter reconstruction
//!   - open returns identical bytes
//!   - repeated delete is idempotent

use arcanum_core::traits::{ByteStream, OperationPayloadStore};
use arcanum_core::types::OperationId;
use arcanum_core::{ArcanumError, Result};
use arcanum_ingestion::operations::payload::local::LocalOperationPayloadStore;
use arcanum_ingestion::operations::payload::s3::{LocalFsObjectStore, S3OperationPayloadStore};
use bytes::Bytes;
use futures::StreamExt;
use std::sync::Arc;

fn one_shot(bytes: Vec<u8>) -> ByteStream {
    Box::pin(futures::stream::once(async move {
        Ok::<_, ArcanumError>(Bytes::from(bytes))
    }))
}

async fn read_all(mut stream: ByteStream) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    while let Some(chunk) = stream.next().await {
        out.extend_from_slice(&chunk?);
    }
    Ok(out)
}

async fn run_payload_contract<S, F, Fut>(new_store: F)
where
    S: OperationPayloadStore,
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = S>,
{
    let store = new_store().await;
    let op_id = OperationId::new();
    let payload: Vec<u8> = b"durable payload bytes \x00\x01\x02 end".to_vec();

    // stage
    let locator = store
        .stage(&op_id, one_shot(payload.clone()))
        .await
        .expect("stage");
    assert!(!locator.is_empty(), "locator must be non-empty");

    // open returns identical bytes
    let opened = read_all(store.open(&locator).await.expect("open"))
        .await
        .expect("read");
    assert_eq!(opened, payload, "open must return identical bytes");

    // staged bytes survive adapter reconstruction
    let store2 = new_store().await;
    let opened2 = read_all(store2.open(&locator).await.expect("open reconstructed"))
        .await
        .expect("read reconstructed");
    assert_eq!(opened2, payload, "staged bytes must survive reconstruction");

    // repeated delete is idempotent
    store.delete(&locator).await.expect("delete #1");
    store
        .delete(&locator)
        .await
        .expect("delete #2 (idempotent)");

    // after delete, open fails as NotFound
    let res = store2.open(&locator).await;
    match res {
        Err(ArcanumError::NotFound(_)) => {}
        Err(other) => panic!("expected NotFound after delete, got {other}"),
        Ok(_) => panic!("expected open after delete to fail"),
    }
}

#[tokio::test]
async fn local_payload_store_contract() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path().to_path_buf();
    run_payload_contract(|| async { LocalOperationPayloadStore::new(root.clone()) }).await;
}

#[tokio::test]
async fn s3_payload_store_contract() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path().to_path_buf();
    let bucket = "arcanum-contract-bucket".to_string();
    let factory = || async {
        S3OperationPayloadStore::new(
            Arc::new(LocalFsObjectStore::new(root.clone())),
            bucket.clone(),
        )
    };

    run_payload_contract(&factory).await;

    // S3-specific: keys live under `operations/{operation_id}`, the locator is
    // `s3://{bucket}/operations/{operation_id}`, and server-side encryption is
    // requested (observable as a `<key>.sse` marker in the file-backed store).
    let store = factory().await;
    let op_id = OperationId::new();
    let locator = store
        .stage(&op_id, one_shot(b"hello".to_vec()))
        .await
        .expect("s3 stage");
    assert_eq!(
        locator,
        format!("s3://{bucket}/operations/{}", op_id.0),
        "S3 locator must be s3://{bucket}/operations/<operation_id>"
    );

    let key_file = root.join("operations").join(op_id.0.to_string());
    assert!(
        key_file.exists(),
        "S3 key file must exist at operations/<operation_id>"
    );
    assert_eq!(
        tokio::fs::read(&key_file).await.expect("read key"),
        b"hello"
    );

    let sse_marker = root.join(format!("operations/{}.sse", op_id.0));
    assert!(
        sse_marker.exists(),
        "server-side-encryption marker must be written for the staged object"
    );

    // repeated delete idempotent on the S3 path as well
    store.delete(&locator).await.expect("s3 delete #1");
    store
        .delete(&locator)
        .await
        .expect("s3 delete #2 (idempotent)");
    assert!(!key_file.exists(), "key file must be gone after delete");
    assert!(!sse_marker.exists(), "sse marker must be gone after delete");
}
