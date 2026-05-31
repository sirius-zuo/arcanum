use arcanum_core::{config::ArcanumConfig, types::*, Result, ArcanumError};
use arcanum_retrieval::{RetrievalOrchestrator, OrchestratorMode, QueryCache};
use std::sync::Arc;
use crate::audit::{AuditLogger, AuditEntry};
use crate::auth::{AuthMiddleware, ApiKeyClaims};

pub struct RetrievalService {
    audit: Arc<AuditLogger>,
    auth: Arc<AuthMiddleware>,
    orchestrator: Arc<RetrievalOrchestrator>,
    cache: Option<Arc<QueryCache>>,
}

impl std::fmt::Debug for RetrievalService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RetrievalService").finish_non_exhaustive()
    }
}

impl RetrievalService {
    pub fn new(_config: ArcanumConfig, audit: Arc<AuditLogger>, auth: Arc<AuthMiddleware>) -> Self {
        let orchestrator = Arc::new(RetrievalOrchestrator::new(OrchestratorMode::QueryClassified));
        Self { audit, auth, orchestrator, cache: None }
    }

    pub fn with_cache(mut self, cache: Arc<QueryCache>) -> Self {
        self.cache = Some(cache);
        self
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

        // Check cache first if available.
        let cache_key = QueryCache::cache_key(&query);
        if let Some(cache) = &self.cache {
            if let Some(cached) = cache.get(&cache_key) {
                return Ok(cached);
            }
        }

        let result = self.orchestrator.retrieve(&query).await?;

        // Store in cache if available.
        if let Some(cache) = &self.cache {
            cache.insert(cache_key, result.clone());
        }

        self.audit.log(AuditEntry {
            operation: "search".into(),
            user_id: claims.user_id.clone(),
            collection_id: collection_id.0.clone(),
            result: "ok".into(),
        }).await;
        Ok(result)
    }
}
