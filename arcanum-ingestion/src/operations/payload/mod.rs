//! Durable operation-payload adapters.
//!
//! Payloads are keyed by their operation, never by a temporary filename or
//! staging key; the returned locator is what `IngestionSubmission` stores.

pub mod local;
pub mod s3;

pub use local::LocalOperationPayloadStore;
pub use s3::{LocalFsObjectStore, S3ObjectStore, S3OperationPayloadStore};
