use arcanum_core::{config::ArcanumConfig, Result};
use std::sync::Arc;
use crate::{
    audit::AuditLogger,
    event_bus::EventBus,
    services::{ingestion::IngestionService, retrieval::RetrievalService, collection::CollectionService},
};

#[derive(Debug)]
pub struct ArcanumEngine {
    pub config: ArcanumConfig,
    pub ingestion: Arc<IngestionService>,
    pub retrieval: Arc<RetrievalService>,
    pub collection: Arc<CollectionService>,
    pub audit: Arc<AuditLogger>,
    pub events: Arc<EventBus>,
}

pub struct ArcanumEngineBuilder {
    config: ArcanumConfig,
}

impl ArcanumEngineBuilder {
    pub fn new(config: ArcanumConfig) -> Self { Self { config } }

    pub async fn build(self) -> Result<Arc<ArcanumEngine>> {
        self.config.validate()?;
        let audit = Arc::new(AuditLogger::new());
        let events = Arc::new(EventBus::new());
        let ingestion = Arc::new(IngestionService::new(self.config.clone(), events.clone(), audit.clone()));
        let retrieval = Arc::new(RetrievalService::new(self.config.clone(), audit.clone()));
        let collection = Arc::new(CollectionService::new(self.config.clone(), audit.clone()));
        Ok(Arc::new(ArcanumEngine { config: self.config, ingestion, retrieval, collection, audit, events }))
    }
}
