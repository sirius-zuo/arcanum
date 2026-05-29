use arcanum_core::{config::ArcanumConfig, Result, ArcanumError};
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
    auth_secret: Option<String>,
}

impl ArcanumEngineBuilder {
    /// Create a builder. Call `with_auth_secret` or set `ARCANUM_AUTH_SECRET` env var
    /// before calling `build()`. No hardcoded default is provided.
    pub fn new(config: ArcanumConfig) -> Self {
        Self { config, auth_secret: None }
    }

    pub fn with_auth_secret(mut self, secret: impl Into<String>) -> Self {
        self.auth_secret = Some(secret.into());
        self
    }

    pub async fn build(self) -> Result<Arc<ArcanumEngine>> {
        self.config.validate()?;

        // Resolve auth secret: explicit > env var > error (no hardcoded fallback).
        let secret = self.auth_secret
            .or_else(|| std::env::var("ARCANUM_AUTH_SECRET").ok())
            .ok_or_else(|| ArcanumError::Config(
                "ARCANUM_AUTH_SECRET must be set via with_auth_secret() or the ARCANUM_AUTH_SECRET env var".into()
            ))?;
        if secret.len() < 32 {
            return Err(ArcanumError::Config(
                "ARCANUM_AUTH_SECRET must be at least 32 characters".into()
            ));
        }
        let auth = Arc::new(AuthMiddleware::new(&secret));
        let audit = Arc::new(AuditLogger::new());
        let events = Arc::new(EventBus::new());
        let ingestion = Arc::new(IngestionService::new(self.config.clone(), events.clone(), audit.clone()));
        let retrieval = Arc::new(RetrievalService::new(self.config.clone(), audit.clone(), auth.clone()));
        let collection = Arc::new(CollectionService::new(self.config.clone(), audit.clone(), auth.clone()));
        Ok(Arc::new(ArcanumEngine { config: self.config, ingestion, retrieval, collection, audit, events, auth }))
    }
}
