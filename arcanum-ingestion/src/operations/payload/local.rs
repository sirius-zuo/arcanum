use arcanum_core::{
    traits::{ByteStream, OperationPayloadStore},
    types::OperationId,
    ArcanumError, Result,
};
use async_trait::async_trait;
use futures::StreamExt;
use std::path::PathBuf;
use tokio::io::AsyncWriteExt;
use tracing::instrument;

/// Filesystem-backed `OperationPayloadStore`.
///
/// Keys are `{operation_id}` under a configured NON-PUBLIC root. The durable
/// locator is a `file://` URI, so it survives adapter reconstruction and stays
/// valid for any process that can read the same root.
pub struct LocalOperationPayloadStore {
    root: PathBuf,
}

impl LocalOperationPayloadStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    fn path_for(&self, operation_id: &OperationId) -> PathBuf {
        self.root.join(operation_id.0.to_string())
    }

    fn path_from_locator(&self, locator: &str) -> Result<PathBuf> {
        let raw = locator
            .strip_prefix("file://")
            .ok_or_else(|| ArcanumError::Storage(format!("invalid payload locator: {locator}")))?;
        Ok(PathBuf::from(raw))
    }
}

#[async_trait]
impl OperationPayloadStore for LocalOperationPayloadStore {
    #[instrument(skip(self, bytes), fields(store = "local_payload", op_id = %operation_id.0), err)]
    async fn stage(&self, operation_id: &OperationId, mut bytes: ByteStream) -> Result<String> {
        tokio::fs::create_dir_all(&self.root)
            .await
            .map_err(|e| ArcanumError::Storage(format!("create payload root: {e}")))?;
        let path = self.path_for(operation_id);
        // Write to a temp file first so a failed/interrupted stream never leaves
        // a partial object under the durable key.
        let tmp = self.root.join(format!("{}.tmp", operation_id.0));
        let mut file = tokio::fs::File::create(&tmp)
            .await
            .map_err(|e| ArcanumError::Storage(format!("create payload temp: {e}")))?;
        while let Some(chunk) = bytes.next().await {
            let chunk = chunk?;
            file.write_all(&chunk)
                .await
                .map_err(|e| ArcanumError::Storage(format!("write payload: {e}")))?;
        }
        file.flush()
            .await
            .map_err(|e| ArcanumError::Storage(format!("flush payload: {e}")))?;
        drop(file);
        tokio::fs::rename(&tmp, &path)
            .await
            .map_err(|e| ArcanumError::Storage(format!("finalize payload: {e}")))?;
        Ok(format!("file://{}", path.display()))
    }

    #[instrument(skip(self), fields(store = "local_payload", locator), err)]
    async fn open(&self, locator: &str) -> Result<ByteStream> {
        let path = self.path_from_locator(locator)?;
        let data = tokio::fs::read(&path)
            .await
            .map_err(|e| ArcanumError::NotFound(format!("payload not found: {locator}: {e}")))?;
        Ok(Box::pin(futures::stream::once(async move {
            Ok(bytes::Bytes::from(data))
        })))
    }

    #[instrument(skip(self), fields(store = "local_payload", locator), err)]
    async fn delete(&self, locator: &str) -> Result<()> {
        let path = self.path_from_locator(locator)?;
        match tokio::fs::remove_file(&path).await {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()), // idempotent
            Err(e) => Err(ArcanumError::Storage(format!("delete payload: {e}"))),
        }
    }
}
