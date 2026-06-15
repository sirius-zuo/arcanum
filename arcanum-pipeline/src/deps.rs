use arcanum_core::traits::{TextEnricher, Embedder, VectorStore, GraphStore, TreeStore,
                            CacheInvalidationBroadcaster, DocumentVersionStore, SnapshotStore, DocumentRegistry};
use arcanum_core::types::{PerBackendChunkers, ShadowContext};
use arcanum_ingestion::{LoaderRegistry, PreprocessorRegistry};
use arcanum_middleware::{RetryPolicy, CircuitBreaker};
use std::sync::Arc;

pub struct PipelineDeps {
    pub loaders:           Arc<LoaderRegistry>,
    pub preprocessors:     Arc<PreprocessorRegistry>,
    pub chunkers:          PerBackendChunkers,
    pub shadow:            Option<ShadowContext>,
    pub context_enricher:  Option<Arc<dyn TextEnricher>>,
    pub entity_extractor:  Option<Arc<dyn TextEnricher>>,
    pub embedder:          Arc<dyn Embedder>,
    pub vector_store:      Arc<dyn VectorStore>,
    pub graph_store:       Option<Arc<dyn GraphStore>>,
    pub tree_store:        Option<Arc<dyn TreeStore>>,
    pub version_store:     Arc<dyn DocumentVersionStore>,
    pub snapshot_store:    Arc<dyn SnapshotStore>,
    /// Kept for backwards compat — engine uses this until Task 15 migration.
    pub document_registry: Arc<dyn DocumentRegistry>,
    pub retry_policy:      RetryPolicy,
    pub cache_invalidator: Arc<CacheInvalidationBroadcaster>,
    pub embedding_cb:      Arc<CircuitBreaker>,
    pub vector_store_cb:   Arc<CircuitBreaker>,
}
