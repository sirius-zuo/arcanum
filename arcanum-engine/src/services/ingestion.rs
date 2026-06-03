use arcanum_core::{types::*, Result};
use arcanum_ingestion::DocumentHashTracker;
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
    hash_tracker: Arc<DocumentHashTracker>,
}

impl std::fmt::Debug for IngestionService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IngestionService").finish_non_exhaustive()
    }
}

impl IngestionService {
    pub fn new(events: Arc<EventBus>, audit: Arc<AuditLogger>) -> Self {
        Self::new_from_parts(
            Arc::new(BoundedQueue::new(10_000)),
            Arc::new(DocumentHashTracker::new()),
            events,
            audit,
        )
    }

    /// Full constructor used by ArcanumEngine::build() to share the queue with workers.
    pub fn new_from_parts(
        queue: Arc<BoundedQueue<IngestionTask>>,
        hash_tracker: Arc<DocumentHashTracker>,
        events: Arc<EventBus>,
        audit: Arc<AuditLogger>,
    ) -> Self {
        Self { queue, events, audit, hash_tracker }
    }

    pub fn new_with_tracker(hash_tracker: Arc<DocumentHashTracker>) -> Self {
        Self::new_from_parts(
            Arc::new(BoundedQueue::new(10_000)),
            hash_tracker,
            Arc::new(EventBus::new()),
            Arc::new(AuditLogger::new()),
        )
    }

    #[instrument(skip(self, req), fields(user_id, source_uri = %req.source_uri, collection_id = ?req.collection_id), err)]
    pub async fn ingest(&self, req: IngestRequest, user_id: &str) -> Result<OperationId> {
        if !req.force && self.hash_tracker.ever_seen(&req.source_uri).await {
            let op_id = OperationId::new();
            self.events.publish("ingestion:progress", serde_json::json!({
                "operation_id": op_id.0,
                "status": "skipped",
                "reason": "already_seen"
            })).await;
            return Ok(op_id);
        }

        let op_id = OperationId::new();
        let task = IngestionTask {
            operation_id: op_id.clone(),
            source_uri: req.source_uri.clone(),
            collection_id: req.collection_id.clone(),
            pipeline_template: req.pipeline_template.unwrap_or("standard".into()),
            attempt: 0,
            force: req.force,
            content: req.content.clone(),
            mime_hint: req.mime_hint.clone(),
        };
        self.queue.push(task).await?;
        self.audit.log(AuditEntry {
            operation: "ingest".into(),
            user_id: user_id.to_string(),
            collection_id: req.collection_id.0,
            result: "accepted".into(),
        }).await;
        self.events.publish("ingestion:progress", serde_json::json!({
            "operation_id": op_id.0,
            "status": "queued"
        })).await;
        Ok(op_id)
    }
}
