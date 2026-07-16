use arcanum_core::{
    config::{ArcanumConfig, OrchestrationMode as CfgMode},
    traits::{VectorStore, Embedder, TextEnricher, GraphStore, TreeStore, SecretStore,
             CacheInvalidationBroadcaster, LexicalIndex, IngestionDepsOverrideResolver,
             SnapshotStore, DocumentVersionStore, ChunkMetadataStore, EvidenceResolver, GcWorker,
             Preprocessor, Reranker},
    types::{RetrievalStrategy, EnrichIntent},
    Result, ArcanumError,
};
use arcanum_ingestion::LocalSnapshotStore;
use arcanum_graph::GraphQueryPlanner;
use arcanum_ingestion::{LoaderRegistry, PreprocessorCatalog,
                        RawLoader, FileLoader, HttpLoader,
                        DoclingPreprocessor, DoclingBackend, PostgresChunkMetadataStore, PostgresExperimentStore};
use arcanum_middleware::{CircuitBreaker, RetryPolicy, BoundedQueue};
use arcanum_pipeline::{PipelineDeps, ArcanumPipelineRegistry, worker::IngestionWorker};
use arcanum_retrieval::{RetrievalOrchestrator, OrchestratorMode,
                        VectorRetriever, GraphRetriever, RaptorRetriever, Bm25Retriever,
                        ColBertRetriever, QueryTransformer, QueryCache};
use arcanum_vector::Bm25Index;
use arcanum_evidence::{DefaultEvidenceResolver, PostgresGcWorker};
use std::{collections::HashMap, sync::Arc, time::Duration};
use crate::{
    audit::AuditLogger,
    auth::AuthMiddleware,
    event_bus::EventBus,
    ingestion_deps_resolver::resolve_chunkers,
    rate_limit::RateLimiter,
    services::{
        admin::AdminService,
        eval::EvalService,
        experiment::{ExperimentService, ExperimentStore, InMemoryExperimentStore},
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
    pub experiment: Arc<ExperimentService>,
    pub audit: Arc<AuditLogger>,
    pub events: Arc<EventBus>,
    pub auth: Arc<AuthMiddleware>,
    pub eval: Arc<EvalService>,
    pub source: Arc<IngestionSourceService>,
    pub admin: Arc<AdminService>,
    pub embedding_cb: Arc<CircuitBreaker>,
    pub vector_store_cb: Arc<CircuitBreaker>,
    pub rate_limiter: Arc<RateLimiter>,
    pub secret_store: Option<Arc<dyn SecretStore>>,
    /// Optional knowledge graph store, exposed for the /api/v1/graph endpoint.
    pub graph_store: Option<Arc<dyn GraphStore>>,
    pub vector_store: Option<Arc<dyn VectorStore>>,
    pub tree_store: Option<Arc<dyn TreeStore>>,
    pub version_store: Arc<dyn DocumentVersionStore>,
    pub snapshot_store: Arc<dyn SnapshotStore>,
    pub chunk_metadata_store: Option<Arc<dyn ChunkMetadataStore>>,
    pub evidence: Option<Arc<dyn EvidenceResolver>>,
    pub gc_worker: Option<Arc<dyn GcWorker>>,
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

/// Composes the primary embedder with any additional embedders into a single
/// `Arc<dyn Embedder>`. With no additional embedders, returns `primary`
/// unchanged. Otherwise all embedders must share one `dimension()` — a
/// mismatch is a Config error — and the composed result round-robins `embed`
/// calls across `[primary, ...additional]` via `EmbeddingParallelismRouter`.
/// Never constructs the router with an empty vec (it panics).
fn compose_embedder(
    primary: Arc<dyn Embedder>,
    additional: Vec<Arc<dyn Embedder>>,
) -> Result<Arc<dyn Embedder>> {
    if additional.is_empty() {
        return Ok(primary);
    }
    let mut all: Vec<Arc<dyn Embedder>> = vec![primary];
    all.extend(additional);
    let dim = all[0].dimension();
    if let Some(bad) = all.iter().find(|e| e.dimension() != dim) {
        return Err(ArcanumError::Config(format!(
            "all embedders must share one dimension; primary is {} but another reports {}",
            dim, bad.dimension())));
    }
    Ok(Arc::new(arcanum_models::EmbeddingParallelismRouter::new(all)))
}

pub struct ArcanumEngineBuilder {
    config: ArcanumConfig,
    auth_secret: Option<String>,
    vector_store: Option<Arc<dyn VectorStore>>,
    embedder: Option<Arc<dyn Embedder>>,
    enricher: Option<Arc<dyn TextEnricher>>,
    named_enrichers: HashMap<String, Arc<dyn TextEnricher>>,
    graph_store: Option<Arc<dyn GraphStore>>,
    tree_store: Option<Arc<dyn TreeStore>>,
    secret_store: Option<Arc<dyn SecretStore>>,
    bm25_index: Option<Arc<Bm25Index>>,
    version_store: Option<Arc<dyn DocumentVersionStore>>,
    snapshot_store: Option<Arc<dyn SnapshotStore>>,
    chunk_metadata_store: Option<Arc<dyn ChunkMetadataStore>>,
    evidence: Option<Arc<dyn EvidenceResolver>>,
    gc_worker: Option<Arc<dyn GcWorker>>,
    experiment_store: Option<Arc<dyn ExperimentStore>>,
    preprocessor_overrides: Vec<(String, Arc<dyn Preprocessor>)>,
    query_transformer: Option<Arc<dyn QueryTransformer>>,
    reranker: Option<Arc<dyn Reranker>>,
    dedup_threshold: Option<f32>,
    additional_embedders: Vec<Arc<dyn Embedder>>,
}


impl Default for ArcanumEngineBuilder {
    fn default() -> Self {
        Self {
            config: ArcanumConfig::default(),
            auth_secret: None,
            vector_store: None,
            embedder: None,
            enricher: None,
            named_enrichers: HashMap::new(),
            graph_store: None,
            tree_store: None,
            secret_store: None,
            bm25_index: None,
            version_store: None,
            snapshot_store: None,
            chunk_metadata_store: None,
            evidence: None,
            gc_worker: None,
            experiment_store: None,
            preprocessor_overrides: Vec::new(),
            query_transformer: None,
            reranker: None,
            dedup_threshold: None,
            additional_embedders: Vec::new(),
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

    /// Registers an additional embedding provider. When one or more are present,
    /// build() wraps the primary embedder plus all additional embedders in an
    /// `EmbeddingParallelismRouter` that round-robins `embed` calls across them.
    /// All embedders (primary + additional) must share one `dimension()` —
    /// otherwise build() fails with a Config error. Not set by default.
    pub fn additional_embedder(mut self, e: Arc<dyn Embedder>) -> Self {
        self.additional_embedders.push(e);
        self
    }

    pub fn enricher(mut self, enricher: Arc<dyn TextEnricher>) -> Self {
        self.enricher = Some(enricher);
        self
    }

    /// Registers an enricher under `name` so `EnrichmentConfig`'s per-intent
    /// provider names (e.g. `entity_extraction_provider`) can route to it.
    pub fn named_enricher(mut self, name: impl Into<String>, provider: Arc<dyn TextEnricher>) -> Self {
        self.named_enrichers.insert(name.into(), provider);
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

    pub fn version_store(mut self, store: Arc<dyn DocumentVersionStore>) -> Self {
        self.version_store = Some(store);
        self
    }

    pub fn snapshot_store(mut self, store: Arc<dyn SnapshotStore>) -> Self {
        self.snapshot_store = Some(store);
        self
    }

    pub fn chunk_metadata_store(mut self, store: Arc<dyn ChunkMetadataStore>) -> Self {
        self.chunk_metadata_store = Some(store);
        self
    }

    pub fn evidence(mut self, resolver: Arc<dyn EvidenceResolver>) -> Self {
        self.evidence = Some(resolver);
        self
    }

    pub fn gc_worker(mut self, worker: Arc<dyn GcWorker>) -> Self {
        self.gc_worker = Some(worker);
        self
    }

    pub fn experiment_store(mut self, store: Arc<dyn ExperimentStore>) -> Self {
        self.experiment_store = Some(store);
        self
    }

    pub fn register_preprocessor(mut self, name: impl Into<String>, p: Arc<dyn Preprocessor>) -> Self {
        self.preprocessor_overrides.push((name.into(), p));
        self
    }

    /// Fans queries out (e.g. HyDE, multi-query) before retrieval. Not set by
    /// default — retrieval runs against the caller's original query only.
    pub fn query_transformer(mut self, t: Arc<dyn QueryTransformer>) -> Self {
        self.query_transformer = Some(t);
        self
    }

    /// Reorders/rescoring pass applied to fused results. Not set by default —
    /// results keep RRF fusion order.
    pub fn reranker(mut self, r: Arc<dyn Reranker>) -> Self {
        self.reranker = Some(r);
        self
    }

    /// Enables cross-document near-duplicate removal on the final result set,
    /// at the given cosine similarity threshold. Not set by default.
    pub fn dedup_threshold(mut self, threshold: f32) -> Self {
        self.dedup_threshold = Some(threshold);
        self
    }

    /// Resolves the enricher used for ingestion's context prefix / entity
    /// extraction steps. With no per-intent provider names set in
    /// `config.enrichment`, this is exactly `self.enricher` (today's
    /// behavior, untouched). Once any provider name is set, a default
    /// enricher is required and an `EnrichmentDispatcher` is built that
    /// routes each named intent to its registered `named_enricher`, falling
    /// back to the default for unnamed intents. An unknown provider name is
    /// always a hard config error — never a silent fallback.
    fn resolve_enricher(&self) -> Result<Option<Arc<dyn TextEnricher>>> {
        let ec = &self.config.enrichment;
        let intent_names = [
            (EnrichIntent::ContextPrefix,   &ec.context_prefix_provider),
            (EnrichIntent::ExtractEntities, &ec.entity_extraction_provider),
            (EnrichIntent::Summarize,       &ec.summarize_provider),
            (EnrichIntent::Caption,         &ec.caption_provider),
        ];
        let any_named = intent_names.iter().any(|(_, n)| n.is_some());
        if !any_named {
            return Ok(self.enricher.clone());
        }
        let default = self.enricher.clone().ok_or_else(|| ArcanumError::Config(
            "enrichment providers are named in config but no default enricher is set — \
             call .enricher(...) with the fallback provider".into()))?;
        let mut dispatcher = arcanum_models::EnrichmentDispatcher::new(default);
        for (intent, name) in intent_names {
            if let Some(name) = name {
                let provider = self.named_enrichers.get(name).cloned().ok_or_else(|| {
                    ArcanumError::Config(format!(
                        "enrichment config names provider '{}' but no enricher was registered \
                         under that name — call .named_enricher(\"{}\", ...)", name, name))
                })?;
                dispatcher = dispatcher.with_override(intent, provider);
            }
        }
        Ok(Some(Arc::new(dispatcher)))
    }

    pub async fn build(self) -> Result<Arc<ArcanumEngine>> {
        self.config.validate()?;

        // Resolved unconditionally (not just when pipeline workers are wired) so an
        // unknown provider name in enrichment config always fails build().
        let enricher = self.resolve_enricher()?;

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

        let embedding_cb    = Arc::new(CircuitBreaker::new("embedding", 5, Duration::from_secs(30)));
        let vector_store_cb = Arc::new(CircuitBreaker::new("vector_store", 5, Duration::from_secs(30)));
        let rate_limiter    = Arc::new(RateLimiter::with_window(120, Duration::from_secs(60)));

        // Build the preprocessor catalog: docling (if configured) registers as
        // "default"; builder-supplied overrides apply on top and can replace
        // "default" itself or add additional named preprocessors for specific
        // collections. Built unconditionally — the engine must build and serve
        // queries even with no preprocessor configured at all; only ingestion
        // tasks fail later if nothing resolves.
        let mut preprocessor_catalog = PreprocessorCatalog::new();
        if let Some(dc) = &self.config.ingestion.docling {
            let backend = DoclingBackend::from(&dc.backend);
            preprocessor_catalog.register("default", Arc::new(DoclingPreprocessor::new(backend)) as Arc<dyn Preprocessor>);
        }
        for (name, p) in &self.preprocessor_overrides {
            preprocessor_catalog.register(name.clone(), p.clone());
        }
        let preprocessor_catalog = Arc::new(preprocessor_catalog);

        let version_store: Arc<dyn DocumentVersionStore> = self.version_store.clone().ok_or_else(|| ArcanumError::Config(
            "version_store is required — call .version_store(Arc::new(SqliteDocumentVersionStore::open(path).await?)) \
             for local/dev or .version_store(Arc::new(PostgresDocumentVersionStore::new(url).await?)) for production".into()
        ))?;

        // Collection and experiment services — needed early for per-job resolver.
        let collection = Arc::new(CollectionService::new(self.config.clone(), audit.clone(), auth.clone(), preprocessor_catalog.clone()));

        // Auto-wire experiment store from storage.database_url when the builder didn't
        // supply one directly. Builder-supplied stores always win.
        let experiment_store: Arc<dyn ExperimentStore> = match (&self.experiment_store, &self.config.storage.database_url) {
            (Some(s), _) => s.clone(),
            (None, Some(url)) => Arc::new(
                PostgresExperimentStore::new(url).await
                    .map_err(|e| ArcanumError::Config(format!(
                        "storage.database_url is set but PostgresExperimentStore failed: {}", e)))?,
            ),
            (None, None) => Arc::new(InMemoryExperimentStore::new()),
        };

        let experiment = Arc::new(ExperimentService::new(
            collection.clone(),
            experiment_store,
        ));

        // Shared queue — passed to both IngestionService (push) and workers (pop).
        let queue = Arc::new(BoundedQueue::new("ingestion", self.config.ingestion.queue_capacity));

        let ingestion = Arc::new(IngestionService::new_from_parts(
            queue.clone(),
            events.clone(),
            audit.clone(),
        ));

        // Per-job deps resolver: enables per-collection chunker overrides and
        // active shadow experiment resolution for each ingestion worker.
        use crate::ingestion_deps_resolver::EngineIngestionDepsResolver;
        let deps_resolver = Arc::new(EngineIngestionDepsResolver {
            collection_service: collection.clone(),
            experiment_service: experiment.clone(),
            global_chunking:    self.config.ingestion.chunking.clone(),
            preprocessor_catalog: preprocessor_catalog.clone(),
        }) as Arc<dyn IngestionDepsOverrideResolver>;

        let snapshot_store: Arc<dyn SnapshotStore> = self.snapshot_store
            .clone()
            .unwrap_or_else(|| {
                Arc::new(LocalSnapshotStore::new("/tmp/arcanum-snapshots")) as Arc<dyn SnapshotStore>
            });

        // Auto-wire a Postgres chunk-metadata store from storage.database_url when the
        // builder didn't supply one directly. Builder-supplied stores always win.
        let chunk_metadata_store: Option<Arc<dyn ChunkMetadataStore>> =
            match (&self.chunk_metadata_store, &self.config.storage.database_url) {
                (Some(s), _) => Some(s.clone()),
                (None, Some(url)) => Some(Arc::new(
                    PostgresChunkMetadataStore::new(url).await
                        .map_err(|e| ArcanumError::Config(format!(
                            "storage.database_url is set but PostgresChunkMetadataStore failed: {}", e)))?,
                ) as Arc<dyn ChunkMetadataStore>),
                (None, None) => None,
            };

        let query_cache: Option<Arc<QueryCache>> =
            self.config.retrieval.query_cache.as_ref().map(|qc| {
                Arc::new(QueryCache::new(
                    qc.max_entries, Duration::from_secs(qc.ttl_secs),
                ))
            });
        let mut invalidators: Vec<Arc<dyn arcanum_core::traits::CacheInvalidator>> = vec![];
        if let Some(c) = &query_cache { invalidators.push(c.clone()); }

        // Wire pipeline workers if embedder + vector_store are available.
        if let (Some(embedder), Some(vector_store)) = (&self.embedder, &self.vector_store) {
            let embedder: Arc<dyn Embedder> = compose_embedder(embedder.clone(), self.additional_embedders.clone())?;
            let embedder: Arc<dyn Embedder> = match &self.config.embedding.cache_redis_url {
                Some(url) => {
                    let cache = Arc::new(arcanum_models::EmbeddingCache::new(
                        url, &self.config.embedding.model_id, embedder.dimension(),
                    ).await?);
                    invalidators.push(cache.clone());
                    Arc::new(arcanum_models::CachingEmbedder::new(embedder.clone(), cache))
                }
                None => embedder.clone(),
            };
            let deps = Arc::new(PipelineDeps {
                loaders: Arc::new(
                    LoaderRegistry::new()
                        .register(Arc::new(RawLoader::new()))
                        .register(Arc::new(FileLoader::new()))
                        .register(Arc::new(HttpLoader::new())),
                ),
                preprocessors:     preprocessor_catalog.get("default"),
                chunkers:          resolve_chunkers(None, &self.config.ingestion.chunking)?,
                shadow:            None,
                context_enricher:  enricher.clone(),
                entity_extractor:  enricher.clone(),
                embedder:          embedder.clone(),
                vector_store:      vector_store.clone(),
                graph_store:       self.graph_store.clone(),
                tree_store:        self.tree_store.clone(),
                version_store:     version_store.clone(),
                snapshot_store:    snapshot_store.clone(),
                chunk_metadata:    chunk_metadata_store.clone(),
                bm25_index:        self.bm25_index.clone(),
                retry_policy:      RetryPolicy::new(
                    self.config.ingestion.retry_max_attempts,
                    self.config.ingestion.retry_base_delay_ms,
                    5_000,
                ),
                cache_invalidator: Arc::new(CacheInvalidationBroadcaster::new(invalidators.clone())),
                embedding_cb:      embedding_cb.clone(),
                vector_store_cb:   vector_store_cb.clone(),
            });
            let registry = Arc::new(ArcanumPipelineRegistry::default());
            let emitter: Arc<dyn arcanum_core::traits::ProgressEmitter> = events.clone();
            for _ in 0..self.config.ingestion.worker_pool_size {
                let worker = IngestionWorker::new(
                    registry.clone(), deps.clone(), emitter.clone(), queue.clone(),
                ).with_resolver(deps_resolver.clone());
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

        // Auto-wire a Postgres GC worker from storage.database_url when the builder
        // didn't supply one directly, and vector/tree/graph/chunk-metadata stores are
        // all present. Builder-supplied workers always win.
        let gc_worker: Option<Arc<dyn GcWorker>> = match (&self.gc_worker, &self.config.storage.database_url) {
            (Some(w), _) => Some(w.clone()),
            (None, Some(url)) => {
                match (&self.vector_store, &self.tree_store, &self.graph_store, &chunk_metadata_store) {
                    (Some(vs), Some(ts), Some(gs), Some(cms)) => Some(Arc::new(
                        PostgresGcWorker::new(
                            url, version_store.clone(), snapshot_store.clone(),
                            vs.clone(), ts.clone(), gs.clone(), cms.clone(),
                        ).await?,
                    ) as Arc<dyn GcWorker>),
                    _ => {
                        tracing::warn!(
                            "storage.database_url set but gc worker not auto-wired: \
                             requires vector, tree, graph, and chunk-metadata stores"
                        );
                        None
                    }
                }
            }
            (None, None) => None,
        };

        // Build RetrievalOrchestrator with whichever retrievers are available.
        let orch_mode = match self.config.retrieval.orchestration_mode {
            CfgMode::Static          => OrchestratorMode::Static(vec![
                RetrievalStrategy::Vector,
                RetrievalStrategy::Bm25,
            ]),
            CfgMode::QueryClassified  => OrchestratorMode::QueryClassified,
            CfgMode::ParallelFusion   => OrchestratorMode::ParallelFusion,
        };
        if matches!(self.config.retrieval.orchestration_mode, CfgMode::Static)
            && self.bm25_index.is_none()
        {
            tracing::warn!(
                "orchestration_mode=Static includes Bm25 but no bm25_index was provided; \
                 BM25 strategy will be silently inactive"
            );
        }
        let mut orchestrator = RetrievalOrchestrator::new(orch_mode);
        let mut retriever_count = 0;
        if let (Some(vs), Some(emb)) = (&self.vector_store, &self.embedder) {
            orchestrator = orchestrator
                .add_retriever(Arc::new(VectorRetriever::new(vs.clone(), emb.clone())));
            orchestrator = orchestrator
                .add_retriever(Arc::new(ColBertRetriever::new(vs.clone(), emb.clone())));
            retriever_count += 2;
        }
        if let (Some(gs), Some(vs), Some(emb), Some(enricher)) = (
            &self.graph_store, &self.vector_store, &self.embedder, &self.enricher,
        ) {
            let planner: Arc<dyn arcanum_core::traits::GraphPlanner> =
                Arc::new(GraphQueryPlanner::new(enricher.clone(), 2));
            orchestrator = orchestrator
                .add_retriever(Arc::new(GraphRetriever::new(
                    gs.clone(), vs.clone(), planner, emb.clone(), 2,
                )));
            retriever_count += 1;
        }
        if let (Some(ts), Some(emb)) = (&self.tree_store, &self.embedder) {
            orchestrator = orchestrator
                .add_retriever(Arc::new(RaptorRetriever::new(ts.clone(), emb.clone(), 3)));
            retriever_count += 1;
        }
        if let Some(bm25) = &self.bm25_index {
            orchestrator = orchestrator.add_retriever(Arc::new(
                Bm25Retriever::new_global(bm25.clone() as Arc<dyn LexicalIndex>)
            ));
            retriever_count += 1;
        }
        metrics::gauge!("arcanum_active_retrievers").set(retriever_count as f64);

        if let Some(t) = &self.query_transformer {
            orchestrator = orchestrator.with_query_transformer(t.clone());
        }
        if let Some(r) = &self.reranker {
            orchestrator = orchestrator.with_reranker(r.clone());
        }
        if let Some(threshold) = self.dedup_threshold {
            orchestrator = orchestrator.with_dedup_threshold(threshold);
        }

        let mut retrieval_svc = RetrievalService::new(
            Arc::new(orchestrator),
            auth.clone(),
            audit.clone(),
            vector_store_cb.clone(),
        );
        if let Some(c) = &query_cache { retrieval_svc = retrieval_svc.with_cache(c.clone()); }
        let retrieval = Arc::new(retrieval_svc);
        let eval       = Arc::new(EvalService::new());
        let source     = Arc::new(IngestionSourceService::new());
        let admin      = Arc::new(AdminService::new(audit.clone()));

        let secret_store = self.secret_store.clone();
        if let Some(store) = &secret_store {
            let store = store.clone();
            let interval_secs = self.config.admin.secret_store_reload_interval_secs;
            tokio::spawn(async move {
                let mut ticker = tokio::time::interval(
                    Duration::from_secs(interval_secs)
                );
                ticker.tick().await; // skip immediate first tick
                loop {
                    ticker.tick().await;
                    if let Err(e) = store.reload().await {
                        tracing::warn!("SecretStore reload failed: {}", e);
                    }
                }
            });
        }

        // Background experiment eval loop
        {
            let exp_svc = experiment.clone();
            let eval_interval = std::time::Duration::from_secs(3600); // 1 hour default
            tokio::spawn(async move {
                loop {
                    tokio::time::sleep(eval_interval).await;
                    let active = exp_svc.active_experiments().await;
                    for (collection_id, exp) in active {
                        tracing::info!(
                            collection_id = %collection_id,
                            experiment_id = %exp.id.0,
                            "running background eval for active experiment"
                        );
                        // TODO: run benchmark against primary and shadow namespaces
                        // Requires: access to vector store for retrieval + labeled queries
                        // Labeled queries for background eval come from the collection's
                        // benchmark query store (to be defined when persistent storage lands).
                        // For now, log and skip — manual promote via API is the path.
                        tracing::info!(
                            "background eval stub: use POST /collections/{}/experiments/{}/eval to trigger manually",
                            collection_id, exp.id.0
                        );
                    }
                }
            });
        }

        Ok(Arc::new(ArcanumEngine {
            config: self.config,
            ingestion,
            retrieval,
            collection,
            experiment,
            audit,
            events,
            auth,
            eval,
            source,
            admin,
            embedding_cb,
            vector_store_cb,
            rate_limiter,
            secret_store,
            graph_store: self.graph_store.clone(),
            vector_store: self.vector_store.clone(),
            tree_store: self.tree_store.clone(),
            version_store: version_store.clone(),
            snapshot_store,
            chunk_metadata_store: chunk_metadata_store.clone(),
            evidence: self.evidence.clone().or_else(|| {
                chunk_metadata_store.as_ref().map(|cms| Arc::new(DefaultEvidenceResolver::new(
                    cms.clone(), version_store.clone(),
                    self.tree_store.clone(), self.graph_store.clone(),
                )) as Arc<dyn EvidenceResolver>)
            }),
            gc_worker,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arcanum_core::{
        types::*,
        traits::{VectorStore, VectorQuery, ScoredChunk, Embedder, TextEnricher},
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
        async fn delete_by_source_uri(&self, _: &str, _: &str) -> AResult<()> { Ok(()) }
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

    /// Echoes its tag into the returned text so which provider handled a
    /// request is observable from the result.
    struct TaggingEnricher(&'static str);
    #[async_trait]
    impl TextEnricher for TaggingEnricher {
        async fn enrich(&self, _req: EnrichRequest) -> AResult<EnrichedText> {
            Ok(EnrichedText(self.0.into()))
        }
    }

    #[tokio::test]
    async fn enrichment_config_routes_intents_to_named_providers() {
        let mut config = ArcanumConfig::default();
        config.enrichment.entity_extraction_provider = Some("gliner".into());
        let builder = ArcanumEngine::builder()
            .config(config)
            .auth_secret("a-32-char-secret-for-testing-ok!")
            .version_store(Arc::new(arcanum_core::traits::NoOpDocumentVersionStore))
            .enricher(Arc::new(TaggingEnricher("default")))
            .named_enricher("gliner", Arc::new(TaggingEnricher("gliner")));

        let enricher = builder.resolve_enricher()
            .expect("resolve_enricher should succeed")
            .expect("an enricher should be resolved once a provider name is set");

        let entities = enricher.enrich(EnrichRequest {
            text: "irrelevant".into(),
            intent: EnrichIntent::ExtractEntities,
            context: None,
        }).await.expect("enrich should succeed");
        assert_eq!(entities.0, "gliner", "ExtractEntities should route to the named 'gliner' provider");

        let summary = enricher.enrich(EnrichRequest {
            text: "irrelevant".into(),
            intent: EnrichIntent::Summarize,
            context: None,
        }).await.expect("enrich should succeed");
        assert_eq!(summary.0, "default", "Summarize has no named provider and should fall back to the default");
    }

    #[tokio::test]
    async fn unknown_enrichment_provider_name_fails_build() {
        let mut config = ArcanumConfig::default();
        config.enrichment.summarize_provider = Some("no-such-provider".into());
        let err = ArcanumEngine::builder()
            .config(config)
            .auth_secret("a-32-char-secret-for-testing-ok!")
            .version_store(Arc::new(arcanum_core::traits::NoOpDocumentVersionStore))
            .enricher(Arc::new(TaggingEnricher("default")))
            .build().await.err().expect("must fail");
        assert!(err.to_string().contains("no-such-provider"), "error should name the unknown provider: {}", err);
    }

    #[tokio::test]
    async fn no_named_enrichment_providers_leaves_resolve_unchanged() {
        // Default-inert: no provider names set anywhere in config, so
        // resolve_enricher must return exactly self.enricher untouched.
        let config = ArcanumConfig::default();
        let builder = ArcanumEngine::builder()
            .config(config)
            .auth_secret("a-32-char-secret-for-testing-ok!")
            .version_store(Arc::new(arcanum_core::traits::NoOpDocumentVersionStore))
            .enricher(Arc::new(TaggingEnricher("default")));

        let enricher = builder.resolve_enricher()
            .expect("resolve_enricher should succeed")
            .expect("default enricher should be passed through");

        let result = enricher.enrich(EnrichRequest {
            text: "irrelevant".into(),
            intent: EnrichIntent::Summarize,
            context: None,
        }).await.expect("enrich should succeed");
        assert_eq!(result.0, "default");
    }

    #[tokio::test]
    async fn evidence_resolver_auto_wired_when_all_stores_present() {
        let engine = ArcanumEngine::builder()
            .auth_secret("a-32-char-secret-for-testing-ok!")
            .version_store(Arc::new(arcanum_core::traits::NoOpDocumentVersionStore))
            .chunk_metadata_store(Arc::new(arcanum_core::traits::InMemoryChunkMetadataStore::new()))
            .tree_store(Arc::new(arcanum_tree::InMemoryTreeStore::new()))
            .graph_store(Arc::new(arcanum_graph::InMemoryGraphStore::new()))
            .build().await
            .expect("build should succeed");

        assert!(
            engine.evidence.is_some(),
            "evidence resolver should be auto-wired once chunk_metadata_store, \
             tree_store, and graph_store are all configured"
        );
    }

    #[tokio::test]
    async fn evidence_resolver_auto_wired_with_partial_backends() {
        let engine = ArcanumEngine::builder()
            .auth_secret("a-32-char-secret-for-testing-ok!")
            .version_store(Arc::new(arcanum_core::traits::NoOpDocumentVersionStore))
            .chunk_metadata_store(Arc::new(arcanum_core::traits::InMemoryChunkMetadataStore::new()))
            .tree_store(Arc::new(arcanum_tree::InMemoryTreeStore::new()))
            // graph_store deliberately omitted
            .build().await
            .expect("build should succeed");

        assert!(
            engine.evidence.is_some(),
            "evidence resolver should be auto-wired from chunk_metadata_store alone, \
             with tree/graph stores now optional"
        );
    }

    #[tokio::test]
    async fn evidence_resolver_stays_none_without_chunk_metadata_store() {
        let engine = ArcanumEngine::builder()
            .auth_secret("a-32-char-secret-for-testing-ok!")
            .version_store(Arc::new(arcanum_core::traits::NoOpDocumentVersionStore))
            // chunk_metadata_store deliberately omitted
            .tree_store(Arc::new(arcanum_tree::InMemoryTreeStore::new()))
            .graph_store(Arc::new(arcanum_graph::InMemoryGraphStore::new()))
            .build().await
            .expect("build should succeed");

        assert!(
            engine.evidence.is_none(),
            "evidence resolver must not be auto-wired without a chunk_metadata_store"
        );
    }

    #[tokio::test]
    async fn no_database_url_means_no_auto_wiring() {
        // Pins the default-off constraint: with no database_url configured and no
        // stores supplied to the builder, neither chunk_metadata_store nor gc_worker
        // should be auto-wired.
        let engine = ArcanumEngine::builder()
            .config(ArcanumConfig::default())
            .auth_secret("a-32-char-secret-for-testing-ok!")
            .vector_store(Arc::new(FakeVectorStore))
            .embedder(Arc::new(FakeEmbedder))
            .version_store(Arc::new(arcanum_core::traits::NoOpDocumentVersionStore))
            .build().await
            .expect("build should succeed");

        assert!(engine.chunk_metadata_store.is_none(), "chunk_metadata_store must stay unset when database_url is unset");
        assert!(engine.gc_worker.is_none(), "gc_worker must stay unset when database_url is unset");
    }

    #[tokio::test]
    #[ignore] // needs TEST_DATABASE_URL pointing at a reachable Postgres
    async fn database_url_auto_wires_chunk_metadata_and_gc() {
        let mut config = ArcanumConfig::default();
        config.storage.database_url = Some(std::env::var("TEST_DATABASE_URL").expect("set TEST_DATABASE_URL to run this test"));

        let engine = ArcanumEngine::builder()
            .config(config)
            .auth_secret("a-32-char-secret-for-testing-ok!")
            .vector_store(Arc::new(FakeVectorStore))
            .embedder(Arc::new(FakeEmbedder))
            .tree_store(Arc::new(arcanum_tree::InMemoryTreeStore::new()))
            .graph_store(Arc::new(arcanum_graph::InMemoryGraphStore::new()))
            .version_store(Arc::new(arcanum_core::traits::NoOpDocumentVersionStore))
            .build().await
            .expect("build should succeed");

        assert!(engine.chunk_metadata_store.is_some(), "chunk_metadata_store should be auto-wired from storage.database_url");
        assert!(engine.gc_worker.is_some(), "gc_worker should be auto-wired once database_url plus vector/tree/graph/chunk-metadata stores are present");
    }

    #[tokio::test]
    async fn gc_worker_not_auto_wired_when_stores_partial() {
        // Verify the warn+None arm: database_url is set, but chunk-metadata store is
        // supplied via builder (no Postgres connection) and vector/tree/graph stores are
        // missing, so gc_worker must not be auto-wired. Also supply an in-memory
        // experiment_store to avoid experiment_store auto-wiring attempts.
        let mut config = ArcanumConfig::default();
        config.storage.database_url = Some("postgres://unused@localhost/unused".into());

        let engine = ArcanumEngine::builder()
            .config(config)
            .auth_secret("a-32-char-secret-for-testing-ok!")
            .chunk_metadata_store(Arc::new(arcanum_core::traits::InMemoryChunkMetadataStore::new()))
            .experiment_store(Arc::new(InMemoryExperimentStore::new()))
            .version_store(Arc::new(arcanum_core::traits::NoOpDocumentVersionStore))
            .build().await
            .expect("build should succeed even with partial stores");

        assert!(
            engine.gc_worker.is_none(),
            "gc_worker must not be auto-wired when required stores are missing (vector/tree/graph)"
        );
    }

    #[tokio::test]
    async fn test_engine_builder_with_dependencies() {
        let engine = ArcanumEngine::builder()
            .config(ArcanumConfig::default())
            .auth_secret("a-32-char-secret-for-testing-ok!")
            .vector_store(Arc::new(FakeVectorStore))
            .embedder(Arc::new(FakeEmbedder))
            .enricher(Arc::new(FakeEnricher))
            .version_store(Arc::new(arcanum_core::traits::NoOpDocumentVersionStore))
            .build().await;
        assert!(engine.is_ok(), "builder should succeed: {:?}", engine.err());
    }

    #[tokio::test]
    async fn embedding_cache_redis_url_none_leaves_build_unchanged() {
        // Default-off: cache_redis_url is None, so build must take the old,
        // Redis-free path — this is just the existing suite passing unchanged.
        let mut config = ArcanumConfig::default();
        config.embedding.cache_redis_url = None;
        let engine = ArcanumEngine::builder()
            .config(config)
            .auth_secret("a-32-char-secret-for-testing-ok!")
            .vector_store(Arc::new(FakeVectorStore))
            .embedder(Arc::new(FakeEmbedder))
            .enricher(Arc::new(FakeEnricher))
            .version_store(Arc::new(arcanum_core::traits::NoOpDocumentVersionStore))
            .build().await;
        assert!(engine.is_ok(), "builder should succeed with cache_redis_url unset: {:?}", engine.err());
    }

    #[tokio::test]
    #[ignore] // requires a live Redis server
    async fn embedding_cache_redis_url_wires_caching_embedder() {
        let url = std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://localhost:6379".to_string());
        let mut config = ArcanumConfig::default();
        config.embedding.cache_redis_url = Some(url);
        let engine = ArcanumEngine::builder()
            .config(config)
            .auth_secret("a-32-char-secret-for-testing-ok!")
            .vector_store(Arc::new(FakeVectorStore))
            .embedder(Arc::new(FakeEmbedder))
            .enricher(Arc::new(FakeEnricher))
            .version_store(Arc::new(arcanum_core::traits::NoOpDocumentVersionStore))
            .build().await;
        assert!(engine.is_ok(), "builder should succeed when cache_redis_url points at a live Redis: {:?}", engine.err());
    }

    struct OneChunkVectorStore;
    #[async_trait]
    impl VectorStore for OneChunkVectorStore {
        async fn upsert(&self, _c: &str, _chunks: Vec<IndexedChunk>) -> AResult<()> { Ok(()) }
        async fn search(&self, _c: &str, _q: &VectorQuery) -> AResult<Vec<ScoredChunk>> {
            Ok(vec![ScoredChunk {
                chunk: IndexedChunk {
                    chunk: Chunk {
                        id: ChunkId::new(),
                        text: "hello world".into(),
                        document_id: DocumentId::new(),
                        collection_id: CollectionId("col1".into()),
                        position: ChunkPosition { start: 0, end: 11, index: 0 },
                        metadata: ChunkMetadata::default(),
                        provenance: ChunkProvenance::default(),
                    },
                    vector: Vector(vec![0.1, 0.2]),
                    token_vectors: None,
                    store_id: String::new(),
                },
                score: 0.9,
            }])
        }
        async fn delete(&self, _c: &str, _ids: &[ChunkId]) -> AResult<()> { Ok(()) }
        async fn collection_exists(&self, _c: &str) -> AResult<bool> { Ok(true) }
        async fn delete_by_source_uri(&self, _: &str, _: &str) -> AResult<()> { Ok(()) }
    }

    #[tokio::test]
    async fn parallel_fusion_search_includes_colbert_strategy() {
        let engine = ArcanumEngine::builder()
            .config(ArcanumConfig::default())
            .auth_secret("a-32-char-secret-for-testing-ok!")
            .vector_store(Arc::new(OneChunkVectorStore))
            .embedder(Arc::new(FakeEmbedder))
            .version_store(Arc::new(arcanum_core::traits::NoOpDocumentVersionStore))
            .build().await
            .expect("build should succeed");

        let token = engine.auth.generate_admin_key("tester");
        let claims = engine.auth.validate_api_key(&token).unwrap();
        let query = Query::new("hello").with_collection(CollectionId("col1".into()));
        let result = engine.retrieval.search(query, &claims).await.unwrap();

        assert!(
            result.strategy_scores.keys().any(|k| k == "ColBert"),
            "ParallelFusion should include a ColBert-strategy result once vector_store+embedder \
             are configured; strategy_scores keys: {:?}", result.strategy_scores.keys().collect::<Vec<_>>()
        );
    }

    struct CountingVectorStore(Arc<std::sync::atomic::AtomicUsize>);
    #[async_trait]
    impl VectorStore for CountingVectorStore {
        async fn upsert(&self, _c: &str, _chunks: Vec<IndexedChunk>) -> AResult<()> { Ok(()) }
        async fn search(&self, _c: &str, _q: &VectorQuery) -> AResult<Vec<ScoredChunk>> {
            self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(vec![ScoredChunk {
                chunk: IndexedChunk {
                    chunk: Chunk {
                        id: ChunkId::new(),
                        text: "hello world".into(),
                        document_id: DocumentId::new(),
                        collection_id: CollectionId("col1".into()),
                        position: ChunkPosition { start: 0, end: 11, index: 0 },
                        metadata: ChunkMetadata::default(),
                        provenance: ChunkProvenance::default(),
                    },
                    vector: Vector(vec![0.1, 0.2]),
                    token_vectors: None,
                    store_id: String::new(),
                },
                score: 0.9,
            }])
        }
        async fn delete(&self, _c: &str, _ids: &[ChunkId]) -> AResult<()> { Ok(()) }
        async fn collection_exists(&self, _c: &str) -> AResult<bool> { Ok(true) }
        async fn delete_by_source_uri(&self, _: &str, _: &str) -> AResult<()> { Ok(()) }
    }

    #[tokio::test]
    async fn query_cache_config_wires_cache_into_retrieval() {
        let mut config = ArcanumConfig::default();
        config.retrieval.query_cache = Some(arcanum_core::config::QueryCacheConfig {
            max_entries: 10, ttl_secs: 60,
        });
        let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));

        let engine = ArcanumEngine::builder()
            .config(config)
            .auth_secret("a-32-char-secret-for-testing-ok!")
            .vector_store(Arc::new(CountingVectorStore(calls.clone())))
            .embedder(Arc::new(FakeEmbedder))
            .version_store(Arc::new(arcanum_core::traits::NoOpDocumentVersionStore))
            .build().await
            .expect("build should succeed");

        let token = engine.auth.generate_admin_key("tester");
        let claims = engine.auth.validate_api_key(&token).unwrap();
        let query = || Query::new("hello").with_collection(CollectionId("col1".into()));

        engine.retrieval.search(query(), &claims).await.unwrap();
        let calls_after_first = calls.load(std::sync::atomic::Ordering::SeqCst);
        assert!(calls_after_first > 0, "first search should reach the vector store");

        engine.retrieval.search(query(), &claims).await.unwrap();
        assert_eq!(
            calls.load(std::sync::atomic::Ordering::SeqCst), calls_after_first,
            "second identical search should be served from the query cache, not the vector store"
        );
    }

    #[tokio::test]
    async fn test_secret_store_field_is_stored_and_accessible() {
        use arcanum_core::traits::SecretStore;
        use std::sync::atomic::{AtomicUsize, Ordering};

        struct FakeSecretStore { reload_count: Arc<AtomicUsize> }
        #[async_trait]
        impl SecretStore for FakeSecretStore {
            async fn get(&self, _key: &str) -> AResult<String> { Ok("value".into()) }
            async fn reload(&self) -> AResult<()> {
                self.reload_count.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }
        }

        let reload_count = Arc::new(AtomicUsize::new(0));
        let store = Arc::new(FakeSecretStore { reload_count: reload_count.clone() });

        let engine = ArcanumEngine::builder()
            .auth_secret("a-32-char-secret-for-testing-ok!")
            .secret_store(store.clone())
            .version_store(Arc::new(arcanum_core::traits::NoOpDocumentVersionStore))
            .build()
            .await
            .expect("build should succeed");

        assert!(engine.secret_store.is_some(), "secret_store should be stored in ArcanumEngine");
        let val = engine.secret_store.as_ref().unwrap().get("any-key").await.unwrap();
        assert_eq!(val, "value");
    }

    #[tokio::test]
    async fn default_build_uses_in_memory_experiment_store() {
        // Pins the default-off constraint: with no database_url configured and no
        // experiment_store supplied to the builder, the experiment store should be
        // InMemoryExperimentStore. This test uses the InMemoryExperimentStore directly
        // and verifies round-trip of an experiment (existing tests like
        // experiment_test.rs::start_sets_experiment_to_active prove in-memory storage works).
        let engine = ArcanumEngine::builder()
            .config(ArcanumConfig::default())
            .auth_secret("a-32-char-secret-for-testing-ok!")
            .vector_store(Arc::new(FakeVectorStore))
            .embedder(Arc::new(FakeEmbedder))
            .version_store(Arc::new(arcanum_core::traits::NoOpDocumentVersionStore))
            .build().await
            .expect("build should succeed");

        // Verify the experiment service can start an experiment (proving in-memory store works)
        let claims = crate::auth::ApiKeyClaims {
            user_id: "test".into(), allowed_collections: vec![], is_admin: true, exp: 9999999999,
        };
        let col_id = arcanum_core::types::CollectionId("default-inmem-col".into());
        engine.collection.create(col_id.clone(), "test".into(), &claims).await.unwrap();

        let cfg = arcanum_core::types::PerBackendChunkConfig {
            vector: arcanum_core::types::ChunkStrategyConfig {
                strategy: "fixed".to_string(),
                params: serde_json::json!({ "chunk_size": 256, "overlap": 8 }),
            },
            graph: None,
            tree: None,
        };
        let exp = engine.experiment.start(col_id.clone(), cfg).await
            .expect("start should succeed with default in-memory store");

        // Verify the experiment was stored and can be retrieved
        let retrieved = engine.experiment.get(&col_id.0, &exp.id).await
            .expect("get should find the experiment in in-memory store");
        assert_eq!(retrieved.id, exp.id, "retrieved experiment should match the started one");
    }

    struct CountingEmbedder(std::sync::atomic::AtomicUsize, usize);
    #[async_trait]
    impl Embedder for CountingEmbedder {
        async fn embed(&self, texts: Vec<String>) -> AResult<Vec<Vector>> {
            self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(texts.iter().map(|_| Vector(vec![0.0; self.1])).collect())
        }
        fn dimension(&self) -> usize { self.1 }
    }

    #[tokio::test]
    async fn additional_embedders_round_robin() {
        let primary = Arc::new(CountingEmbedder(std::sync::atomic::AtomicUsize::new(0), 2));
        let additional = Arc::new(CountingEmbedder(std::sync::atomic::AtomicUsize::new(0), 2));

        let embedder = compose_embedder(primary.clone(), vec![additional.clone() as Arc<dyn Embedder>])
            .expect("composing same-dimension embedders should succeed");

        embedder.embed(vec!["a".into()]).await.unwrap();
        embedder.embed(vec!["b".into()]).await.unwrap();

        assert_eq!(primary.0.load(std::sync::atomic::Ordering::SeqCst), 1, "primary should be hit exactly once");
        assert_eq!(additional.0.load(std::sync::atomic::Ordering::SeqCst), 1, "additional should be hit exactly once");
    }

    #[tokio::test]
    async fn mismatched_embedder_dimensions_fail_build() {
        let mut config = ArcanumConfig::default();
        config.storage.database_url = None;
        let err = ArcanumEngine::builder()
            .config(config)
            .auth_secret("a-32-char-secret-for-testing-ok!")
            .vector_store(Arc::new(FakeVectorStore))
            .embedder(Arc::new(CountingEmbedder(std::sync::atomic::AtomicUsize::new(0), 3)))
            .additional_embedder(Arc::new(CountingEmbedder(std::sync::atomic::AtomicUsize::new(0), 4)))
            .version_store(Arc::new(arcanum_core::traits::NoOpDocumentVersionStore))
            .build().await
            .err()
            .expect("build must fail on mismatched embedder dimensions");
        assert!(err.to_string().contains("dimension"), "error should mention 'dimension': {}", err);
    }

    #[tokio::test]
    #[ignore] // needs TEST_DATABASE_URL pointing at a reachable Postgres
    async fn experiment_store_auto_wired_from_database_url() {
        // Verify experiments persist across engine restarts when database_url is set.
        // This is the restart-survival property the plan exists for.
        let db_url = std::env::var("TEST_DATABASE_URL")
            .expect("set TEST_DATABASE_URL to run this test");

        // Generate a unique collection ID to avoid conflicts in parallel test runs
        let unique_suffix = uuid::Uuid::new_v4().to_string();
        let col_id = arcanum_core::types::CollectionId(
            format!("experiment-persistence-test-{}", unique_suffix)
        );

        let cfg = arcanum_core::types::PerBackendChunkConfig {
            vector: arcanum_core::types::ChunkStrategyConfig {
                strategy: "fixed".to_string(),
                params: serde_json::json!({ "chunk_size": 256, "overlap": 8 }),
            },
            graph: None,
            tree: None,
        };

        let claims = crate::auth::ApiKeyClaims {
            user_id: "test".into(), allowed_collections: vec![], is_admin: true, exp: 9999999999,
        };

        // Engine 1: Create an experiment with database_url auto-wiring
        {
            let mut config = ArcanumConfig::default();
            config.storage.database_url = Some(db_url.clone());

            let engine1 = ArcanumEngine::builder()
                .config(config)
                .auth_secret("a-32-char-secret-for-testing-ok!")
                .version_store(Arc::new(arcanum_core::traits::NoOpDocumentVersionStore))
                .build().await
                .expect("engine1 build should succeed");

            // Create the collection
            engine1.collection.create(col_id.clone(), "test".into(), &claims).await
                .expect("collection.create should succeed");

            // Start an experiment
            let exp = engine1.experiment.start(col_id.clone(), cfg.clone()).await
                .expect("start should succeed with auto-wired postgres store");

            tracing::info!(experiment_id = %exp.id.0, "experiment started with engine1");
        }

        // Engine 2: Restart on the same database_url and verify experiment persists
        {
            let mut config = ArcanumConfig::default();
            config.storage.database_url = Some(db_url.clone());

            let engine2 = ArcanumEngine::builder()
                .config(config)
                .auth_secret("a-32-char-secret-for-testing-ok!")
                .version_store(Arc::new(arcanum_core::traits::NoOpDocumentVersionStore))
                .build().await
                .expect("engine2 build should succeed");

            // List active experiments to verify persistence
            let active = engine2.experiment.active_experiments().await;
            let found = active.iter()
                .any(|(id, _)| id == &col_id.0);

            assert!(
                found,
                "experiment started with engine1 should be found with engine2 after restart"
            );

            tracing::info!("experiment persisted across restart with engine2");
        }
    }
}

#[cfg(test)]
mod builder_tests {
    use super::*;
    use arcanum_core::config::{ArcanumConfig, OrchestrationMode};

    async fn build_with_mode(mode: OrchestrationMode) -> bool {
        let mut config = ArcanumConfig::default();
        config.retrieval.orchestration_mode = mode;
        ArcanumEngineBuilder::new(config)
            .with_auth_secret("a-32-char-test-secret-for-testing!!")
            .version_store(Arc::new(arcanum_core::traits::NoOpDocumentVersionStore))
            .build()
            .await
            .is_ok()
    }

    #[tokio::test]
    async fn orchestration_mode_from_config_builds() {
        // Each variant must produce a valid engine — if the match were missing an arm
        // or were hardcoded, at least one of these would fail to build or panic.
        assert!(build_with_mode(OrchestrationMode::ParallelFusion).await,  "ParallelFusion must build");
        assert!(build_with_mode(OrchestrationMode::QueryClassified).await, "QueryClassified must build");
        assert!(build_with_mode(OrchestrationMode::Static).await,          "Static must build");
    }
}

#[cfg(test)]
mod resolution_tests {
    use super::resolve_chunkers;
    use arcanum_core::types::{ChunkStrategyConfig, PerBackendChunkConfig};

    fn make_fixed(size: u64, overlap: u64) -> ChunkStrategyConfig {
        ChunkStrategyConfig {
            strategy: "fixed".to_string(),
            params: serde_json::json!({ "chunk_size": size, "overlap": overlap }),
        }
    }

    fn make_semantic(max_chars: u64) -> ChunkStrategyConfig {
        ChunkStrategyConfig {
            strategy: "semantic".to_string(),
            params: serde_json::json!({ "max_chars": max_chars }),
        }
    }

    #[test]
    fn no_collection_override_uses_global_default() {
        let global = PerBackendChunkConfig { vector: make_fixed(512, 64), graph: None, tree: None };
        assert!(resolve_chunkers(None, &global).is_ok());
    }

    #[test]
    fn collection_vector_override_wins() {
        let global = PerBackendChunkConfig { vector: make_fixed(512, 64), graph: None, tree: None };
        let collection = PerBackendChunkConfig {
            vector: make_semantic(800),
            graph:  None,
            tree:   None,
        };
        assert!(resolve_chunkers(Some(&collection), &global).is_ok());
    }

    #[test]
    fn collection_none_graph_falls_back_to_global_then_vector() {
        let global = PerBackendChunkConfig { vector: make_fixed(512, 64), graph: None, tree: None };
        assert!(resolve_chunkers(None, &global).is_ok());
    }

    #[test]
    fn unknown_strategy_in_collection_config_returns_error() {
        let global = PerBackendChunkConfig { vector: make_fixed(512, 64), graph: None, tree: None };
        let bad = PerBackendChunkConfig {
            vector: ChunkStrategyConfig {
                strategy: "nonexistent".to_string(),
                params:   serde_json::json!({}),
            },
            graph: None,
            tree:  None,
        };
        match resolve_chunkers(Some(&bad), &global) {
            Ok(_) => panic!("should have returned error for unknown strategy"),
            Err(e) => assert!(e.to_string().contains("nonexistent")),
        }
    }
}
