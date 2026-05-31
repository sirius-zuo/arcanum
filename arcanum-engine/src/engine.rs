use arcanum_core::{
    config::ArcanumConfig,
    traits::{VectorStore, Embedder, TextEnricher, GraphStore, TreeStore, SecretStore,
             CacheInvalidationBroadcaster},
    Result, ArcanumError,
};
use arcanum_graph::GraphQueryPlanner;
use arcanum_ingestion::{LoaderRegistry, PreprocessorRegistry, DocumentHashTracker,
                        RawLoader, FixedSizeChunker};
use arcanum_middleware::{CircuitBreaker, RetryPolicy, BoundedQueue};
use arcanum_pipeline::{PipelineDeps, ArcanumPipelineRegistry, worker::IngestionWorker};
use arcanum_retrieval::{RetrievalOrchestrator, OrchestratorMode,
                        VectorRetriever, GraphRetriever, RaptorRetriever};
use arcanum_vector::Bm25Index;
use std::{sync::Arc, time::Duration};
use crate::{
    audit::AuditLogger,
    auth::AuthMiddleware,
    event_bus::EventBus,
    services::{
        admin::AdminService,
        eval::EvalService,
        ingestion::IngestionService,
        retrieval::RetrievalService,
        collection::CollectionService,
        source::IngestionSourceService,
    },
};

pub struct ArcanumEngine {
    pub config: ArcanumConfig,
    pub ingestion: Arc<IngestionService>,
    pub retrieval: Arc<RetrievalService>,
    pub collection: Arc<CollectionService>,
    pub audit: Arc<AuditLogger>,
    pub events: Arc<EventBus>,
    pub auth: Arc<AuthMiddleware>,
    pub eval: Arc<EvalService>,
    pub source: Arc<IngestionSourceService>,
    pub admin: Arc<AdminService>,
    pub embedding_cb: Arc<CircuitBreaker>,
    pub vector_store_cb: Arc<CircuitBreaker>,
}

impl std::fmt::Debug for ArcanumEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ArcanumEngine")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

impl ArcanumEngine {
    pub fn builder() -> ArcanumEngineBuilder {
        ArcanumEngineBuilder::default()
    }

    /// Allow request through embedding circuit breaker.
    pub fn check_embedding_circuit(&self) -> bool {
        self.embedding_cb.allow_request()
    }

    /// Record a successful embedding call.
    pub fn record_embedding_success(&self) {
        self.embedding_cb.record_success();
    }

    /// Record a failed embedding call.
    pub fn record_embedding_failure(&self) {
        self.embedding_cb.record_failure();
    }

    /// Allow request through vector store circuit breaker.
    pub fn check_vector_store_circuit(&self) -> bool {
        self.vector_store_cb.allow_request()
    }

    /// Record a successful vector store call.
    pub fn record_vector_store_success(&self) {
        self.vector_store_cb.record_success();
    }

    /// Record a failed vector store call.
    pub fn record_vector_store_failure(&self) {
        self.vector_store_cb.record_failure();
    }
}

pub struct ArcanumEngineBuilder {
    config: ArcanumConfig,
    auth_secret: Option<String>,
    vector_store: Option<Arc<dyn VectorStore>>,
    embedder: Option<Arc<dyn Embedder>>,
    enricher: Option<Arc<dyn TextEnricher>>,
    graph_store: Option<Arc<dyn GraphStore>>,
    tree_store: Option<Arc<dyn TreeStore>>,
    secret_store: Option<Arc<dyn SecretStore>>,
    bm25_index: Option<Arc<Bm25Index>>,
}

impl Default for ArcanumEngineBuilder {
    fn default() -> Self {
        Self {
            config: ArcanumConfig::default(),
            auth_secret: None,
            vector_store: None,
            embedder: None,
            enricher: None,
            graph_store: None,
            tree_store: None,
            secret_store: None,
            bm25_index: None,
        }
    }
}

impl ArcanumEngineBuilder {
    /// Create a builder. Call `with_auth_secret` or set `ARCANUM_AUTH_SECRET` env var
    /// before calling `build()`. No hardcoded default is provided.
    pub fn new(config: ArcanumConfig) -> Self {
        Self { config, ..Self::default() }
    }

    pub fn config(mut self, config: ArcanumConfig) -> Self {
        self.config = config;
        self
    }

    pub fn auth_secret(mut self, secret: impl Into<String>) -> Self {
        self.auth_secret = Some(secret.into());
        self
    }

    /// Kept for backward compatibility.
    pub fn with_auth_secret(self, secret: impl Into<String>) -> Self {
        self.auth_secret(secret)
    }

    pub fn vector_store(mut self, store: Arc<dyn VectorStore>) -> Self {
        self.vector_store = Some(store);
        self
    }

    pub fn embedder(mut self, embedder: Arc<dyn Embedder>) -> Self {
        self.embedder = Some(embedder);
        self
    }

    pub fn enricher(mut self, enricher: Arc<dyn TextEnricher>) -> Self {
        self.enricher = Some(enricher);
        self
    }

    pub fn graph_store(mut self, store: Arc<dyn GraphStore>) -> Self {
        self.graph_store = Some(store);
        self
    }

    pub fn tree_store(mut self, store: Arc<dyn TreeStore>) -> Self {
        self.tree_store = Some(store);
        self
    }

    pub fn secret_store(mut self, store: Arc<dyn SecretStore>) -> Self {
        self.secret_store = Some(store);
        self
    }

    pub fn bm25_index(mut self, index: Arc<Bm25Index>) -> Self {
        self.bm25_index = Some(index);
        self
    }

    pub async fn build(self) -> Result<Arc<ArcanumEngine>> {
        self.config.validate()?;

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

        let auth  = Arc::new(AuthMiddleware::new(&secret));
        let audit = Arc::new(AuditLogger::new());
        let events = Arc::new(EventBus::new());

        let embedding_cb    = Arc::new(CircuitBreaker::new(5, Duration::from_secs(30)));
        let vector_store_cb = Arc::new(CircuitBreaker::new(5, Duration::from_secs(30)));

        // Shared queue and hash tracker — passed to both IngestionService (push) and workers (pop).
        let queue        = Arc::new(BoundedQueue::new(self.config.ingestion.queue_capacity));
        let hash_tracker = Arc::new(DocumentHashTracker::new());

        let ingestion = Arc::new(IngestionService::new_from_parts(
            queue.clone(),
            hash_tracker.clone(),
            events.clone(),
            audit.clone(),
        ));

        // Wire pipeline workers if embedder + vector_store are available.
        if let (Some(embedder), Some(vector_store)) = (&self.embedder, &self.vector_store) {
            let deps = Arc::new(PipelineDeps {
                loaders:           Arc::new(LoaderRegistry::new().register(Arc::new(RawLoader::new()))),
                preprocessors:     Arc::new(PreprocessorRegistry::new()),
                chunker:           Arc::new(FixedSizeChunker::new(512, 64)),
                context_enricher:  self.enricher.clone(),
                entity_extractor:  self.enricher.clone(),
                embedder:          embedder.clone(),
                vector_store:      vector_store.clone(),
                graph_store:       self.graph_store.clone(),
                tree_store:        self.tree_store.clone(),
                hash_tracker:      hash_tracker.clone(),
                retry_policy:      RetryPolicy::new(
                    self.config.ingestion.retry_max_attempts,
                    self.config.ingestion.retry_base_delay_ms,
                    5_000,
                ),
                cache_invalidator: Arc::new(CacheInvalidationBroadcaster::new(vec![])),
                embedding_cb:      embedding_cb.clone(),
                vector_store_cb:   vector_store_cb.clone(),
            });
            let registry = Arc::new(ArcanumPipelineRegistry::default());
            let emitter: Arc<dyn arcanum_core::traits::ProgressEmitter> = events.clone();
            for _ in 0..self.config.ingestion.worker_pool_size {
                let worker = IngestionWorker::new(
                    registry.clone(), deps.clone(), emitter.clone(), queue.clone(),
                );
                tokio::spawn(async move {
                    while let Some(_) = worker.process_next().await {}
                });
            }
        } else {
            tracing::warn!(
                "arcanum-engine: embedder or vector_store not configured — \
                 ingestion workers not started; tasks will queue but not execute"
            );
        }

        // Build RetrievalOrchestrator with whichever retrievers are available.
        let mut orchestrator = RetrievalOrchestrator::new(OrchestratorMode::QueryClassified);
        if let (Some(vs), Some(emb)) = (&self.vector_store, &self.embedder) {
            orchestrator = orchestrator
                .add_retriever(Arc::new(VectorRetriever::new(vs.clone(), emb.clone())));
        }
        let _ = &self.bm25_index; // Bm25Retriever is collection-scoped — wired per-query, not here.
        if let (Some(gs), Some(vs), Some(emb), Some(enricher)) = (
            &self.graph_store, &self.vector_store, &self.embedder, &self.enricher,
        ) {
            let planner = GraphQueryPlanner::new(enricher.clone(), 2);
            orchestrator = orchestrator
                .add_retriever(Arc::new(GraphRetriever::new(
                    gs.clone(), vs.clone(), planner, emb.clone(),
                )));
        }
        if let (Some(ts), Some(emb)) = (&self.tree_store, &self.embedder) {
            orchestrator = orchestrator
                .add_retriever(Arc::new(RaptorRetriever::new(ts.clone(), emb.clone(), 3)));
        }

        let retrieval = Arc::new(RetrievalService::new(
            Arc::new(orchestrator),
            auth.clone(),
            audit.clone(),
            vector_store_cb.clone(),
        ));
        let collection = Arc::new(CollectionService::new(self.config.clone(), audit.clone(), auth.clone()));
        let eval       = Arc::new(EvalService::new());
        let source     = Arc::new(IngestionSourceService::new());
        let admin      = Arc::new(AdminService::new(audit.clone()));

        Ok(Arc::new(ArcanumEngine {
            config: self.config,
            ingestion,
            retrieval,
            collection,
            audit,
            events,
            auth,
            eval,
            source,
            admin,
            embedding_cb,
            vector_store_cb,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arcanum_core::{
        types::*,
        traits::{VectorStore, VectorQuery, ScoredChunk, Embedder, TextEnricher, GraphStore, TreeStore, GraphQuery},
        Result as AResult,
    };
    use async_trait::async_trait;

    struct FakeVectorStore;
    #[async_trait]
    impl VectorStore for FakeVectorStore {
        async fn upsert(&self, _c: &str, _chunks: Vec<IndexedChunk>) -> AResult<()> { Ok(()) }
        async fn search(&self, _c: &str, _q: &VectorQuery) -> AResult<Vec<ScoredChunk>> { Ok(vec![]) }
        async fn delete(&self, _c: &str, _ids: &[ChunkId]) -> AResult<()> { Ok(()) }
        async fn collection_exists(&self, _c: &str) -> AResult<bool> { Ok(false) }
    }

    struct FakeEmbedder;
    #[async_trait]
    impl Embedder for FakeEmbedder {
        async fn embed(&self, texts: Vec<String>) -> AResult<Vec<Vector>> {
            Ok(texts.iter().map(|_| Vector(vec![0.1, 0.2])).collect())
        }
        fn dimension(&self) -> usize { 2 }
    }

    struct FakeEnricher;
    #[async_trait]
    impl TextEnricher for FakeEnricher {
        async fn enrich(&self, req: EnrichRequest) -> AResult<EnrichedText> {
            Ok(EnrichedText(req.text))
        }
    }

    #[tokio::test]
    async fn test_engine_builder_with_dependencies() {
        let engine = ArcanumEngine::builder()
            .config(ArcanumConfig::default())
            .auth_secret("a-32-char-secret-for-testing-ok!")
            .vector_store(Arc::new(FakeVectorStore))
            .embedder(Arc::new(FakeEmbedder))
            .enricher(Arc::new(FakeEnricher))
            .build().await;
        assert!(engine.is_ok(), "builder should succeed: {:?}", engine.err());
    }
}
