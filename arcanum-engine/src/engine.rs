use arcanum_core::{config::ArcanumConfig, Result};
use std::sync::Arc;
use crate::{
    audit::AuditLogger,
    auth::AuthMiddleware,
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
    pub auth: Arc<AuthMiddleware>,
}

pub struct ArcanumEngineBuilder {
    config: ArcanumConfig,
    auth_secret: String,
}

impl ArcanumEngineBuilder {
    pub fn new(config: ArcanumConfig) -> Self {
        // In production, secret should come from SecretStore / env. Default is dev-only.
        Self { config, auth_secret: "dev-secret-change-in-production".into() }
    }

    pub fn with_auth_secret(mut self, secret: impl Into<String>) -> Self {
        self.auth_secret = secret.into();
        self
    }

    pub async fn build(self) -> Result<Arc<ArcanumEngine>> {
        self.config.validate()?;
        let auth = Arc::new(AuthMiddleware::new(&self.auth_secret));
        let audit = Arc::new(AuditLogger::new());
        let events = Arc::new(EventBus::new());
        let ingestion = Arc::new(IngestionService::new(self.config.clone(), events.clone(), audit.clone()));
        let retrieval = Arc::new(RetrievalService::new(self.config.clone(), audit.clone(), auth.clone()));
        let collection = Arc::new(CollectionService::new(self.config.clone(), audit.clone(), auth.clone()));
        Ok(Arc::new(ArcanumEngine { config: self.config, ingestion, retrieval, collection, audit, events, auth }))
    }
}
