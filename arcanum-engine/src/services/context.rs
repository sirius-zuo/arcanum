use crate::audit::{AuditEntry, AuditLogger};
use crate::auth::{ApiKeyClaims, AuthMiddleware};
use arcanum_context::{assemble, resolve_query, AssembleParams, ConversationRewriter};
use arcanum_core::{
    config::ContextConfig,
    traits::{ChunkMetadataStore, TokenCounter},
    types::*,
    ArcanumError,
};
use arcanum_middleware::CircuitBreaker;
use arcanum_retrieval::{hydrate::hydrate_sources, RetrievalOrchestrator};
use std::sync::Arc;

const DEFAULT_BACKGROUND_SHARE: f32 = 0.2;

#[derive(Debug)]
pub enum ContextError {
    Invalid(String),
    Forbidden(String),
    Unavailable(String),
    Internal(ArcanumError),
}

impl std::fmt::Display for ContextError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ContextError::Invalid(m)
            | ContextError::Forbidden(m)
            | ContextError::Unavailable(m) => f.write_str(m),
            ContextError::Internal(e) => write!(f, "{e}"),
        }
    }
}

pub struct ContextService {
    orchestrator: Arc<RetrievalOrchestrator>,
    registry: Arc<dyn ChunkMetadataStore>,
    rewriter: Option<Arc<dyn ConversationRewriter>>,
    counter: Arc<dyn TokenCounter>,
    config: ContextConfig,
    auth: Arc<AuthMiddleware>,
    audit: Arc<AuditLogger>,
    vector_store_cb: Arc<CircuitBreaker>,
}

impl std::fmt::Debug for ContextService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ContextService").finish_non_exhaustive()
    }
}

impl ContextService {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        orchestrator: Arc<RetrievalOrchestrator>,
        registry: Arc<dyn ChunkMetadataStore>,
        rewriter: Option<Arc<dyn ConversationRewriter>>,
        counter: Arc<dyn TokenCounter>,
        config: ContextConfig,
        auth: Arc<AuthMiddleware>,
        audit: Arc<AuditLogger>,
        vector_store_cb: Arc<CircuitBreaker>,
    ) -> Self {
        Self {
            orchestrator,
            registry,
            rewriter,
            counter,
            config,
            auth,
            audit,
            vector_store_cb,
        }
    }

    pub async fn assemble(
        &self,
        req: ContextRequest,
        claims: &ApiKeyClaims,
    ) -> Result<ContextResponse, ContextError> {
        req.validate().map_err(ContextError::Invalid)?;
        let collection_id = req.collection_id.clone();
        if !self.auth.can_access_collection(claims, &collection_id) {
            return Err(ContextError::Forbidden(format!(
                "not authorised to access collection '{collection_id}'"
            )));
        }
        if !self.vector_store_cb.allow_request() {
            return Err(ContextError::Unavailable(
                "circuit open: vector store unavailable".into(),
            ));
        }

        let resolved = resolve_query(
            req.query.as_deref(),
            req.messages.as_deref(),
            self.rewriter.as_deref(),
        )
        .await;
        let query = Query::new(resolved.text.clone())
            .with_collection(CollectionId(collection_id.clone()))
            .with_top_k(req.candidate_k.unwrap_or(self.config.default_candidate_k));
        let candidates = match self.orchestrator.retrieve_candidates(&query).await {
            Ok(c) if !c.lists.is_empty() => {
                self.vector_store_cb.record_success();
                c
            }
            Ok(c) => {
                // Only a vector-store-backed strategy failure counts against
                // the shared breaker; an empty set with no active strategy or
                // a non-vector failure must not trip it for `search`.
                if c.failed.iter().any(|(s, _)| {
                    matches!(s, RetrievalStrategy::Vector | RetrievalStrategy::ColBert)
                }) {
                    self.vector_store_cb.record_failure();
                }
                return Err(ContextError::Unavailable("retrieval unavailable".into()));
            }
            Err(e) => {
                self.vector_store_cb.record_failure();
                return Err(ContextError::Internal(e));
            }
        };
        let candidates = hydrate_sources(self.registry.as_ref(), &collection_id, candidates)
            .await
            .map_err(ContextError::Internal)?;

        let params = AssembleParams {
            token_budget: req.token_budget.unwrap_or(self.config.default_token_budget),
            background_share: req.background_share.unwrap_or(DEFAULT_BACKGROUND_SHARE),
            render: req.render,
        };
        let assembled = assemble(&candidates, &params, self.counter.as_ref());

        let mut strategies_ok: Vec<String> = Vec::new();
        for list in &candidates.lists {
            let name = strategy_name(&list.strategy);
            if !strategies_ok.iter().any(|s| s == name) {
                strategies_ok.push(name.to_string());
            }
        }
        let retrieval = RetrievalInfo {
            queries: candidates.queries.clone(),
            strategies_ok,
            strategies_failed: candidates
                .failed
                .iter()
                .map(|(s, reason)| StrategyFailure {
                    strategy: strategy_name(s).to_string(),
                    reason: reason.clone(),
                })
                .collect(),
        };

        self.audit
            .log(AuditEntry {
                operation: "context".into(),
                user_id: claims.user_id.clone(),
                collection_id,
                result: "ok".into(),
            })
            .await;

        Ok(ContextResponse {
            resolved_query: resolved.text,
            resolved_query_source: resolved.source,
            passages: assembled.passages,
            background: assembled.background,
            usage: assembled.usage,
            retrieval,
            rendered: assembled.rendered,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::test_support::{
        seeded_registry, FailingRetriever, RegistryRetriever, DOC_TEXT,
    };
    use arcanum_core::traits::Retriever;
    use arcanum_retrieval::OrchestratorMode;
    use std::time::Duration;

    struct Fixture {
        svc: ContextService,
        auth: Arc<AuthMiddleware>,
        audit: Arc<AuditLogger>,
        cb: Arc<CircuitBreaker>,
    }

    async fn fixture(
        retrievers: impl FnOnce(&Chunk) -> Vec<Arc<dyn Retriever>>,
        cb: Arc<CircuitBreaker>,
    ) -> Fixture {
        let (registry, chunk) = seeded_registry().await;
        let mut orchestrator = RetrievalOrchestrator::new(OrchestratorMode::ParallelFusion);
        for r in retrievers(&chunk) {
            orchestrator = orchestrator.add_retriever(r);
        }
        let auth = Arc::new(AuthMiddleware::new("a-32-char-secret-for-testing-ok!"));
        let audit = Arc::new(AuditLogger::new());
        let svc = ContextService::new(
            Arc::new(orchestrator),
            registry,
            None,
            Arc::new(arcanum_core::traits::ApproxCl100kCounter::new()),
            ContextConfig::default(),
            auth.clone(),
            audit.clone(),
            cb.clone(),
        );
        Fixture {
            svc,
            auth,
            audit,
            cb,
        }
    }

    fn vector_ok(chunk: &Chunk) -> Vec<Arc<dyn Retriever>> {
        vec![Arc::new(RegistryRetriever {
            chunk: chunk.clone(),
            strategy: RetrievalStrategy::Vector,
        })]
    }

    fn closed_cb() -> Arc<CircuitBreaker> {
        Arc::new(CircuitBreaker::new(
            "vector_store",
            5,
            Duration::from_secs(30),
        ))
    }

    fn admin(auth: &AuthMiddleware) -> ApiKeyClaims {
        auth.validate_api_key(&auth.generate_admin_key("tester"))
            .unwrap()
    }

    fn req(query: &str) -> ContextRequest {
        ContextRequest {
            collection_id: "col1".into(),
            query: Some(query.into()),
            messages: None,
            token_budget: None,
            background_share: None,
            candidate_k: None,
            render: None,
        }
    }

    #[tokio::test]
    async fn invalid_request_is_invalid() {
        let f = fixture(vector_ok, closed_cb()).await;
        let err = f
            .svc
            .assemble(req("   "), &admin(&f.auth))
            .await
            .unwrap_err();
        assert!(matches!(err, ContextError::Invalid(_)), "{err:?}");
    }

    #[tokio::test]
    async fn inaccessible_collection_is_forbidden() {
        let f = fixture(vector_ok, closed_cb()).await;
        let token = f.auth.generate_api_key("u", vec!["other".into()]);
        let claims = f.auth.validate_api_key(&token).unwrap();
        let err = f.svc.assemble(req("fox"), &claims).await.unwrap_err();
        assert!(matches!(err, ContextError::Forbidden(_)), "{err:?}");
    }

    #[tokio::test]
    async fn open_circuit_is_unavailable() {
        let cb = closed_cb();
        for _ in 0..5 {
            cb.record_failure();
        }
        let f = fixture(vector_ok, cb).await;
        let err = f
            .svc
            .assemble(req("fox"), &admin(&f.auth))
            .await
            .unwrap_err();
        assert!(
            matches!(&err, ContextError::Unavailable(m) if m == "circuit open: vector store unavailable"),
            "{err:?}"
        );
    }

    #[tokio::test]
    async fn failed_vector_strategy_is_unavailable_and_records_failure() {
        let cb = Arc::new(CircuitBreaker::new(
            "vector_store",
            1,
            Duration::from_secs(30),
        ));
        let f = fixture(
            |_| vec![Arc::new(FailingRetriever(RetrievalStrategy::Vector)) as Arc<dyn Retriever>],
            cb,
        )
        .await;
        let err = f
            .svc
            .assemble(req("fox"), &admin(&f.auth))
            .await
            .unwrap_err();
        assert!(
            matches!(&err, ContextError::Unavailable(m) if m == "retrieval unavailable"),
            "{err:?}"
        );
        assert!(
            !f.cb.allow_request(),
            "a failure must be recorded, opening a threshold-1 breaker"
        );
    }

    #[tokio::test]
    async fn empty_candidates_without_failures_leave_breaker_closed() {
        let cb = Arc::new(CircuitBreaker::new(
            "vector_store",
            2,
            Duration::from_secs(30),
        ));
        let f = fixture(|_| vec![], cb).await;
        for _ in 0..5 {
            let err = f
                .svc
                .assemble(req("fox"), &admin(&f.auth))
                .await
                .unwrap_err();
            assert!(
                matches!(&err, ContextError::Unavailable(m) if m == "retrieval unavailable"),
                "{err:?}"
            );
        }
        assert!(
            f.cb.allow_request(),
            "no active strategy must not record breaker failures"
        );
    }

    #[tokio::test]
    async fn empty_candidates_with_non_vector_failure_leave_breaker_closed() {
        let cb = Arc::new(CircuitBreaker::new(
            "vector_store",
            1,
            Duration::from_secs(30),
        ));
        let f = fixture(
            |_| vec![Arc::new(FailingRetriever(RetrievalStrategy::Bm25)) as Arc<dyn Retriever>],
            cb,
        )
        .await;
        for _ in 0..3 {
            let err = f
                .svc
                .assemble(req("fox"), &admin(&f.auth))
                .await
                .unwrap_err();
            assert!(matches!(err, ContextError::Unavailable(_)), "{err:?}");
        }
        assert!(
            f.cb.allow_request(),
            "a BM25-only failure must not trip the vector-store breaker"
        );
    }

    #[tokio::test]
    async fn partial_failure_returns_ok_with_strategies_failed() {
        let f = fixture(
            |c| {
                let mut v = vector_ok(c);
                v.push(Arc::new(FailingRetriever(RetrievalStrategy::Bm25)));
                v
            },
            closed_cb(),
        )
        .await;
        let resp = f.svc.assemble(req("fox"), &admin(&f.auth)).await.unwrap();
        assert_eq!(resp.retrieval.strategies_failed.len(), 1);
        assert_eq!(resp.retrieval.strategies_failed[0].strategy, "bm25");
        assert_eq!(resp.retrieval.strategies_ok, vec!["vector".to_string()]);
        assert_eq!(resp.passages.len(), 1);
        assert_eq!(resp.passages[0].text, DOC_TEXT);
    }

    #[tokio::test]
    async fn successful_call_writes_context_audit_entry() {
        let f = fixture(vector_ok, closed_cb()).await;
        f.svc.assemble(req("fox"), &admin(&f.auth)).await.unwrap();
        let records = f.audit.query(10).await;
        assert_eq!(records.len(), 1);
        let e = &records[0].entry;
        assert_eq!(
            (
                e.operation.as_str(),
                e.result.as_str(),
                e.collection_id.as_str()
            ),
            ("context", "ok", "col1")
        );
        assert_eq!(e.user_id, "tester");
    }

    #[tokio::test]
    async fn applies_config_defaults() {
        let f = fixture(vector_ok, closed_cb()).await;
        let resp = f.svc.assemble(req("fox"), &admin(&f.auth)).await.unwrap();
        assert_eq!(resp.usage.budget, 4000);
        assert_eq!(resp.resolved_query, "fox");
        assert_eq!(resp.retrieval.queries, vec!["fox".to_string()]);
    }
}
