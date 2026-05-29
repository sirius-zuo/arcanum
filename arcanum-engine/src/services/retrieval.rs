use arcanum_core::{config::ArcanumConfig, types::*, Result, ArcanumError};
use std::sync::Arc;
use crate::audit::{AuditLogger, AuditEntry};
use crate::auth::{AuthMiddleware, ApiKeyClaims};

#[derive(Debug)]
pub struct RetrievalService {
    audit: Arc<AuditLogger>,
    auth: Arc<AuthMiddleware>,
}

impl RetrievalService {
    pub fn new(_config: ArcanumConfig, audit: Arc<AuditLogger>, auth: Arc<AuthMiddleware>) -> Self {
        Self { audit, auth }
    }

    pub async fn search(&self, query: Query, claims: &ApiKeyClaims) -> Result<RetrievalResult> {
        // Verify the caller may access the requested collection.
        let collection_id = query.collection_id.as_ref()
            .ok_or_else(|| ArcanumError::Config("search requires an explicit collection_id".into()))?;
        if !self.auth.can_access_collection(claims, &collection_id.0) {
            return Err(ArcanumError::Auth(format!(
                "not authorised to search collection '{}'", collection_id.0
            )));
        }
        let result = RetrievalResult {
            chunks: vec![], citations: vec![],
            strategy_scores: Default::default(), confidence: 0.0,
        };
        self.audit.log(AuditEntry {
            operation: "search".into(),
            user_id: claims.user_id.clone(),
            collection_id: collection_id.0.clone(),
            result: "ok".into(),
        }).await;
        Ok(result)
    }
}
