use async_trait::async_trait;
use crate::Result;
use crate::types::OperationId;
use bytes::Bytes;
use futures::Stream;
use std::pin::Pin;

/// A durable byte stream used for operation payloads.
pub type ByteStream = Pin<Box<dyn Stream<Item = Result<Bytes>> + Send>>;

/// Durable storage for operation payload bytes, addressed by a stable locator.
///
/// Payloads are keyed by their operation, never by a temporary filename or
/// staging key; the returned locator is what `IngestionSubmission` stores.
#[async_trait]
pub trait OperationPayloadStore: Send + Sync {
    /// Stage payload bytes under the operation and return a durable locator.
    async fn stage(&self, operation_id: &OperationId, bytes: ByteStream) -> Result<String>;

    /// Open the staged bytes for the given locator.
    async fn open(&self, locator: &str) -> Result<ByteStream>;

    /// Delete the staged bytes for the given locator. Repeated delete is idempotent.
    async fn delete(&self, locator: &str) -> Result<()>;
}
