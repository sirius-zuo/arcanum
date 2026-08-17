use arcanum_core::{
    traits::OperationStore,
    types::{CollectionId, IngestionReport, IngestionSubmission, IngestionTask, OperationId},
    Result,
};
use arcanum_middleware::BoundedQueue;
use std::sync::Arc;
use tracing::instrument;
use crate::audit::{AuditLogger, AuditEntry};
use crate::event_bus::EventBus;

#[derive(Debug, Clone)]
pub struct IngestRequest {
    pub source_uri: String,
    pub collection_id: CollectionId,
    pub pipeline_template: Option<String>,
    pub force: bool,
    pub content: Option<Vec<u8>>,
    pub mime_hint: Option<String>,
}

pub struct IngestionService {
    queue: Arc<BoundedQueue<IngestionTask>>,
    events: Arc<EventBus>,
    audit: Arc<AuditLogger>,
    operations: Arc<dyn OperationStore>,
}

impl std::fmt::Debug for IngestionService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IngestionService").finish_non_exhaustive()
    }
}

impl IngestionService {
    pub fn new(
        events: Arc<EventBus>,
        audit: Arc<AuditLogger>,
        operations: Arc<dyn OperationStore>,
    ) -> Self {
        Self {
            queue: Arc::new(BoundedQueue::new("ingestion", 10_000)),
            events,
            audit,
            operations,
        }
    }

    pub fn new_from_parts(
        queue: Arc<BoundedQueue<IngestionTask>>,
        events: Arc<EventBus>,
        audit: Arc<AuditLogger>,
        operations: Arc<dyn OperationStore>,
    ) -> Self {
        Self { queue, events, audit, operations }
    }

    #[instrument(skip(self, req), fields(user_id, source = %req.source_uri, collection_id = %req.collection_id.0), err)]
    pub async fn ingest(&self, req: IngestRequest, user_id: &str) -> Result<OperationId> {
        let start = std::time::Instant::now();
        let source = req.source_uri.clone();
        let pipeline_template = req.pipeline_template.clone().unwrap_or_else(|| "standard".into());

        // Persist `Accepted` BEFORE any work is enqueued. `create_or_get` makes
        // the submission idempotent: a replay returns the ORIGINAL operation.
        let submission = IngestionSubmission {
            idempotency_key: derive_idempotency_key(&req),
            logical_source_uri: req.source_uri.clone(),
            mime_hint: req.mime_hint.clone(),
            collection_id: req.collection_id.clone(),
            pipeline_configuration: serde_json::json!({ "template": pipeline_template }),
            payload: req.content.clone(),
            payload_locator: None,
        };
        let created = self.operations.create_or_get(&submission).await?;
        let op_id = created.operation.operation_id.clone();

        // Idempotent replay: return the existing operation WITHOUT enqueueing a
        // second task.
        if !created.is_new {
            return Ok(op_id);
        }

        let task = IngestionTask {
            operation_id: op_id.clone(),
            source_uri: req.source_uri.clone(),
            collection_id: req.collection_id.clone(),
            pipeline_template,
            attempt: 0,
            force: req.force,
            content: req.content.clone(),
            mime_hint: req.mime_hint.clone(),
        };
        if let Err(err) = self.queue.push(task).await {
            // The submission is durably `Accepted`; a rejected queue means this
            // operation can never run, so persist `Failed` before surfacing the
            // error.
            let report = IngestionReport::queue_rejected(created.operation, &err);
            if let Err(complete_err) = self.operations.complete(&report).await {
                tracing::error!(
                    op_id = %op_id.0,
                    err = %complete_err,
                    "failed to persist QUEUE_REJECTED terminal report"
                );
            }
            return Err(err);
        }

        self.audit.log(AuditEntry {
            operation: "ingest".into(),
            user_id: user_id.to_string(),
            collection_id: req.collection_id.0,
            result: "accepted".into(),
        }).await;
        self.events.publish("ingestion:progress", serde_json::json!({
            "operation_id": op_id.0,
            "status": "accepted"
        })).await;
        let elapsed = start.elapsed().as_secs_f64();
        metrics::counter!("arcanum_ingest_docs_total", "source" => source.clone(), "status" => "ok").increment(1);
        metrics::histogram!("arcanum_ingest_duration_seconds", "source" => source).record(elapsed);
        Ok(op_id)
    }
}

/// Derive a stable idempotency key from the logical submission so identical
/// re-submissions replay to the SAME operation while a change in the logical
/// source, collection, template, force intent, MIME hint, or inline payload
/// creates a distinct one. The key is a SHA-256 digest so it is stable across
/// process restarts (it is stored in the durable `OperationStore`).
fn derive_idempotency_key(req: &IngestRequest) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(req.source_uri.as_bytes());
    hasher.update(b"\0");
    hasher.update(req.collection_id.0.as_bytes());
    hasher.update(b"\0");
    hasher.update(req.pipeline_template.as_deref().unwrap_or("standard").as_bytes());
    hasher.update(b"\0");
    hasher.update([req.force as u8]);
    hasher.update(b"\0");
    if let Some(mime_hint) = &req.mime_hint {
        hasher.update(mime_hint.as_bytes());
    }
    hasher.update(b"\0");
    if let Some(content) = &req.content {
        hasher.update(content);
    }
    format!("sha256:{}", hex::encode(hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use arcanum_core::traits::InMemoryOperationStore;
    use arcanum_core::types::OperationStatus;

    #[test]
    fn idempotency_key_is_stable_and_distinguishes_submissions() {
        let base = IngestRequest {
            source_uri: "s3://b/k".into(),
            collection_id: CollectionId("col".into()),
            pipeline_template: Some("standard".into()),
            force: false,
            content: Some(b"a".to_vec()),
            mime_hint: Some("text/plain".into()),
        };
        let mut same = base.clone();
        same.force = false;
        let mut different_content = base.clone();
        different_content.content = Some(b"b".to_vec());
        let mut different_source = base.clone();
        different_source.source_uri = "s3://b/other".into();
        let mut forced = base.clone();
        forced.force = true;
        let mut different_mime = base.clone();
        different_mime.mime_hint = Some("application/pdf".into());

        assert_eq!(derive_idempotency_key(&base), derive_idempotency_key(&same));
        assert_ne!(
            derive_idempotency_key(&base),
            derive_idempotency_key(&different_content)
        );
        assert_ne!(
            derive_idempotency_key(&base),
            derive_idempotency_key(&different_source)
        );
        assert_ne!(derive_idempotency_key(&base), derive_idempotency_key(&forced));
        assert_ne!(
            derive_idempotency_key(&base),
            derive_idempotency_key(&different_mime),
            "a different MIME hint must yield a distinct idempotency key"
        );
    }

    #[tokio::test]
    async fn ingest_replay_returns_same_operation_without_duplicate_enqueue() {
        let store: Arc<dyn OperationStore> = Arc::new(InMemoryOperationStore::new());
        let svc = IngestionService::new(
            Arc::new(EventBus::new()),
            Arc::new(AuditLogger::new()),
            store.clone(),
        );
        let req = IngestRequest {
            source_uri: "raw://x".into(),
            collection_id: CollectionId("col".into()),
            pipeline_template: Some("standard".into()),
            force: false,
            content: Some(b"hello".to_vec()),
            mime_hint: None,
        };
        let first = svc.ingest(req.clone(), "u").await.expect("first");
        let replay = svc.ingest(req, "u").await.expect("replay");
        assert_eq!(first, replay);
        let stored = store.get(&first).await.expect("get").expect("exists");
        assert_eq!(stored.status, OperationStatus::Accepted);
    }
}
