use arcanum_core::{config::ArcanumConfig, types::*, Result};
use arcanum_middleware::BoundedQueue;
use std::sync::Arc;
use crate::audit::{AuditLogger, AuditEntry};
use crate::event_bus::EventBus;

#[derive(Debug, Clone)]
pub struct IngestionTask {
    pub operation_id: OperationId,
    pub source_uri: String,
    pub collection_id: CollectionId,
    pub pipeline_template: String,
}

#[derive(Debug, Clone)]
pub struct IngestRequest {
    pub source_uri: String,
    pub collection_id: CollectionId,
    pub pipeline_template: Option<String>,
}

pub struct IngestionService {
    queue: Arc<BoundedQueue<IngestionTask>>,
    events: Arc<EventBus>,
    audit: Arc<AuditLogger>,
}

impl std::fmt::Debug for IngestionService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IngestionService").finish_non_exhaustive()
    }
}

impl IngestionService {
    pub fn new(_config: ArcanumConfig, events: Arc<EventBus>, audit: Arc<AuditLogger>) -> Self {
        Self { queue: Arc::new(BoundedQueue::new(10_000)), events, audit }
    }

    pub async fn ingest(&self, req: IngestRequest, user_id: &str) -> Result<OperationId> {
        let op_id = OperationId::new();
        let task = IngestionTask {
            operation_id: op_id.clone(),
            source_uri: req.source_uri.clone(),
            collection_id: req.collection_id.clone(),
            pipeline_template: req.pipeline_template.unwrap_or("standard".into()),
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
