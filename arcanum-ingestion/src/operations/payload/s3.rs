use arcanum_core::{
    traits::{ByteStream, OperationPayloadStore},
    types::OperationId,
    ArcanumError, Result,
};
use async_trait::async_trait;
use futures::StreamExt;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::instrument;

/// Minimal S3-compatible object-store abstraction.
///
/// `S3OperationPayloadStore` is transport-agnostic: any object store that can
/// put/get/delete by key satisfies it. A real AWS S3 / MinIO binding implements
/// this over SigV4 + `x-amz-server-side-encryption`; the file-backed
/// `LocalFsObjectStore` ships in-crate for the contract tests and for local
/// development without a live S3 endpoint.
#[async_trait]
pub trait S3ObjectStore: Send + Sync {
    /// Store `bytes` under `key`. When `server_side_encryption` is true the
    /// object is stored with server-side encryption (the AWS binding sets
    /// `x-amz-server-side-encryption: AES256`).
    async fn put(&self, key: &str, bytes: Vec<u8>, server_side_encryption: bool) -> Result<()>;

    /// Fetch `key`, or `None` if it does not exist.
    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>>;

    /// Delete `key`. Deleting a missing key is a no-op (idempotent).
    async fn delete(&self, key: &str) -> Result<()>;
}

/// S3-backed `OperationPayloadStore`.
///
/// Keys are `operations/{operation_id}` and objects are always stored with
/// server-side encryption. The durable locator is `s3://{bucket}/{key}`.
pub struct S3OperationPayloadStore {
    client: Arc<dyn S3ObjectStore>,
    bucket: String,
}

impl S3OperationPayloadStore {
    pub fn new(client: Arc<dyn S3ObjectStore>, bucket: impl Into<String>) -> Self {
        Self {
            client,
            bucket: bucket.into(),
        }
    }

    fn key_for(&self, operation_id: &OperationId) -> String {
        format!("operations/{}", operation_id.0)
    }

    fn parse_locator(&self, locator: &str) -> Result<String> {
        let rest = locator
            .strip_prefix("s3://")
            .ok_or_else(|| ArcanumError::Storage(format!("invalid s3 locator: {locator}")))?;
        let (bucket, key) = rest
            .split_once('/')
            .ok_or_else(|| ArcanumError::Storage(format!("invalid s3 locator: {locator}")))?;
        if bucket != self.bucket {
            return Err(ArcanumError::Storage(format!(
                "locator bucket mismatch: {locator}"
            )));
        }
        Ok(key.to_string())
    }
}

#[async_trait]
impl OperationPayloadStore for S3OperationPayloadStore {
    #[instrument(skip(self, bytes), fields(store = "s3_payload", op_id = %operation_id.0), err)]
    async fn stage(&self, operation_id: &OperationId, mut bytes: ByteStream) -> Result<String> {
        let mut buf = Vec::new();
        while let Some(chunk) = bytes.next().await {
            buf.extend_from_slice(&chunk?);
        }
        let key = self.key_for(operation_id);
        self.client.put(&key, buf, true).await?;
        Ok(format!("s3://{}/{}", self.bucket, key))
    }

    #[instrument(skip(self), fields(store = "s3_payload", locator), err)]
    async fn open(&self, locator: &str) -> Result<ByteStream> {
        let key = self.parse_locator(locator)?;
        let data = self
            .client
            .get(&key)
            .await?
            .ok_or_else(|| ArcanumError::NotFound(format!("payload not found: {locator}")))?;
        Ok(Box::pin(futures::stream::once(async move {
            Ok(bytes::Bytes::from(data))
        })))
    }

    #[instrument(skip(self), fields(store = "s3_payload", locator), err)]
    async fn delete(&self, locator: &str) -> Result<()> {
        let key = self.parse_locator(locator)?;
        self.client.delete(&key).await
    }
}

/// File-backed S3-compatible object store used by the contract tests and for
/// local development without a real S3/MinIO endpoint.
///
/// Bytes live under `<root>/<key>`. A server-side-encryption request is
/// recorded with a `<key>.sse` marker so the adapter's SSE behavior is
/// observable and testable.
pub struct LocalFsObjectStore {
    root: PathBuf,
}

impl LocalFsObjectStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }
}

#[async_trait]
impl S3ObjectStore for LocalFsObjectStore {
    async fn put(&self, key: &str, bytes: Vec<u8>, server_side_encryption: bool) -> Result<()> {
        let path = self.root.join(key);
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| ArcanumError::Storage(format!("create s3 key dir: {e}")))?;
        }
        tokio::fs::write(&path, &bytes)
            .await
            .map_err(|e| ArcanumError::Storage(format!("s3 put: {e}")))?;
        if server_side_encryption {
            let marker = self.root.join(format!("{key}.sse"));
            tokio::fs::write(&marker, b"aes256")
                .await
                .map_err(|e| ArcanumError::Storage(format!("write sse marker: {e}")))?;
        }
        Ok(())
    }

    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>> {
        let path = self.root.join(key);
        match tokio::fs::read(&path).await {
            Ok(bytes) => Ok(Some(bytes)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(ArcanumError::Storage(format!("s3 get: {e}"))),
        }
    }

    async fn delete(&self, key: &str) -> Result<()> {
        let path = self.root.join(key);
        let _ = tokio::fs::remove_file(&path).await; // missing key is a no-op
        let marker = self.root.join(format!("{key}.sse"));
        let _ = tokio::fs::remove_file(&marker).await;
        Ok(())
    }
}
