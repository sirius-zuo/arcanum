use arcanum_core::{types::*, Result, ArcanumError};
use arcanum_middleware::CircuitBreaker;
use arcanum_retrieval::{RetrievalOrchestrator, QueryCache};
use std::sync::Arc;
use tracing::instrument;
use crate::audit::{AuditLogger, AuditEntry};
use crate::auth::{AuthMiddleware, ApiKeyClaims};

pub struct RetrievalService {
    audit: Arc<AuditLogger>,
    auth: Arc<AuthMiddleware>,
    orchestrator: Arc<RetrievalOrchestrator>,
    cache: Option<Arc<QueryCache>>,
    vector_store_cb: Arc<CircuitBreaker>,
}

impl std::fmt::Debug for RetrievalService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RetrievalService").finish_non_exhaustive()
    }
}

impl RetrievalService {
    pub fn new(
        orchestrator: Arc<RetrievalOrchestrator>,
        auth: Arc<AuthMiddleware>,
        audit: Arc<AuditLogger>,
        vector_store_cb: Arc<CircuitBreaker>,
    ) -> Self {
        Self { audit, auth, orchestrator, cache: None, vector_store_cb }
    }

    pub fn with_cache(mut self, cache: Arc<QueryCache>) -> Self {
        self.cache = Some(cache);
        self
    }

    #[instrument(skip(self, claims), fields(collection_id, top_k = query.top_k), err)]
    pub async fn search(&self, query: Query, claims: &ApiKeyClaims) -> Result<RetrievalResult> {
        let collection_id = query.collection_id.as_ref()
            .ok_or_else(|| ArcanumError::Config("search requires an explicit collection_id".into()))?;
        tracing::Span::current().record("collection_id", &collection_id.0 as &str);
        if !self.auth.can_access_collection(claims, &collection_id.0) {
            return Err(ArcanumError::Auth(format!(
                "not authorised to search collection '{}'", collection_id.0
            )));
        }

        if !self.vector_store_cb.allow_request() {
            return Err(ArcanumError::Retrieval(
                "circuit open: vector store unavailable".into()
            ));
        }

        let cache_key = QueryCache::cache_key(&query);
        if let Some(cache) = &self.cache {
            if let Some(cached) = cache.get(&cache_key) {
                return Ok(cached);
            }
        }

        let result = match self.orchestrator.retrieve(&query).await {
            Ok(r) => {
                self.vector_store_cb.record_success();
                r
            }
            Err(e) => {
                self.vector_store_cb.record_failure();
                return Err(e);
            }
        };

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

#[cfg(test)]
mod tests {
    use super::*;
    use arcanum_core::types::RetrievalStrategy;
    use arcanum_core::traits::Retriever;
    use arcanum_middleware::CircuitBreaker;
    use arcanum_retrieval::{RetrievalOrchestrator, OrchestratorMode};
    use std::time::Duration;

    struct AlwaysOneRetriever;
    #[async_trait::async_trait]
    impl Retriever for AlwaysOneRetriever {
        async fn retrieve(&self, q: &Query) -> arcanum_core::Result<Vec<RetrievedChunk>> {
            let col = q.collection_id.clone().unwrap_or(CollectionId(String::new()));
            Ok(vec![RetrievedChunk {
                indexed_chunk: IndexedChunk {
                    chunk: Chunk {
                        id: ChunkId::new(),
                        text: "stub result".into(),
                        document_id: DocumentId::new(),
                        collection_id: col,
                        position: ChunkPosition { start: 0, end: 11, index: 0 },
                        metadata: ChunkMetadata::default(),
                provenance: Default::default(),
                    },
                    vector: Vector(vec![]),
                    token_vectors: None,
                    store_id: "s1".into(),
                },
                score: 0.9,
                strategy: RetrievalStrategy::Vector,
            }])
        }
        fn strategy(&self) -> RetrievalStrategy { RetrievalStrategy::Vector }
    }

    fn make_service(cb: Arc<CircuitBreaker>) -> (RetrievalService, Arc<AuthMiddleware>) {
        let orchestrator = Arc::new(
            RetrievalOrchestrator::new(OrchestratorMode::ParallelFusion)
                .add_retriever(Arc::new(AlwaysOneRetriever))
        );
        let auth = Arc::new(AuthMiddleware::new("a-32-char-secret-for-testing-ok!"));
        let audit = Arc::new(AuditLogger::new());
        let svc = RetrievalService::new(orchestrator, auth.clone(), audit, cb);
        (svc, auth)
    }

    struct CountingRetriever(Arc<std::sync::atomic::AtomicUsize>);
    #[async_trait::async_trait]
    impl Retriever for CountingRetriever {
        async fn retrieve(&self, q: &Query) -> arcanum_core::Result<Vec<RetrievedChunk>> {
            self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let col = q.collection_id.clone().unwrap_or(CollectionId(String::new()));
            Ok(vec![RetrievedChunk {
                indexed_chunk: IndexedChunk {
                    chunk: Chunk {
                        id: ChunkId::new(),
                        text: "stub result".into(),
                        document_id: DocumentId::new(),
                        collection_id: col,
                        position: ChunkPosition { start: 0, end: 11, index: 0 },
                        metadata: ChunkMetadata::default(),
                        provenance: Default::default(),
                    },
                    vector: Vector(vec![]),
                    token_vectors: None,
                    store_id: "s1".into(),
                },
                score: 0.9,
                strategy: RetrievalStrategy::Vector,
            }])
        }
        fn strategy(&self) -> RetrievalStrategy { RetrievalStrategy::Vector }
    }

    fn make_counting_service() -> (RetrievalService, Arc<AuthMiddleware>, Arc<std::sync::atomic::AtomicUsize>) {
        let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let orchestrator = Arc::new(
            RetrievalOrchestrator::new(OrchestratorMode::ParallelFusion)
                .add_retriever(Arc::new(CountingRetriever(calls.clone())))
        );
        let auth = Arc::new(AuthMiddleware::new("a-32-char-secret-for-testing-ok!"));
        let audit = Arc::new(AuditLogger::new());
        let cb = Arc::new(CircuitBreaker::new("vector_store", 5, Duration::from_secs(30)));
        let svc = RetrievalService::new(orchestrator, auth.clone(), audit, cb);
        (svc, auth, calls)
    }

    #[tokio::test]
    async fn identical_query_is_served_from_cache_on_second_call() {
        let (svc, auth, calls) = make_counting_service();
        let svc = svc.with_cache(Arc::new(QueryCache::new(10, Duration::from_secs(60))));
        let token = auth.generate_admin_key("test-user");
        let claims = auth.validate_api_key(&token).unwrap();
        let q = || Query::new("hello").with_collection(CollectionId("col1".into())).with_top_k(5);

        svc.search(q(), &claims).await.unwrap();
        svc.search(q(), &claims).await.unwrap();

        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1,
            "second identical search must be a cache hit");
    }

    #[tokio::test]
    async fn test_search_with_wired_retriever_returns_results() {
        let cb = Arc::new(CircuitBreaker::new("vector_store", 5, Duration::from_secs(30)));
        let (svc, auth) = make_service(cb);
        let token = auth.generate_admin_key("test-user");
        let claims = auth.validate_api_key(&token).unwrap();

        let query = Query::new("hello").with_collection(CollectionId("col1".into()));
        let result = svc.search(query, &claims).await.unwrap();
        assert_eq!(result.chunks.len(), 1, "should return the stub chunk");
    }

    #[tokio::test]
    async fn test_search_blocked_by_open_circuit_breaker() {
        let cb = Arc::new(CircuitBreaker::new("vector_store", 5, Duration::from_secs(30)));
        for _ in 0..5 { cb.record_failure(); }

        let (svc, auth) = make_service(cb);
        let token = auth.generate_admin_key("test-user");
        let claims = auth.validate_api_key(&token).unwrap();

        let query = Query::new("hello").with_collection(CollectionId("col1".into()));
        let result = svc.search(query, &claims).await;
        assert!(result.is_err(), "open circuit breaker should block search");
        assert!(result.unwrap_err().to_string().contains("circuit"));
    }
}
