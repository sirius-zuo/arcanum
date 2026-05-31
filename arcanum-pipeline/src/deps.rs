use arcanum_core::traits::{Chunker, TextEnricher, Embedder, VectorStore, GraphStore, TreeStore};
use arcanum_ingestion::{LoaderRegistry, PreprocessorRegistry, DocumentHashTracker};
use arcanum_middleware::RetryPolicy;
use std::sync::Arc;

pub struct PipelineDeps {
    pub loaders:          Arc<LoaderRegistry>,
    pub preprocessors:    Arc<PreprocessorRegistry>,
    pub chunker:          Arc<dyn Chunker>,
    pub context_enricher: Option<Arc<dyn TextEnricher>>,
    pub entity_extractor: Option<Arc<dyn TextEnricher>>,
    pub embedder:         Arc<dyn Embedder>,
    pub vector_store:     Arc<dyn VectorStore>,
    pub graph_store:      Option<Arc<dyn GraphStore>>,
    pub tree_store:       Option<Arc<dyn TreeStore>>,
    pub hash_tracker:     Arc<DocumentHashTracker>,
    pub retry_policy:     RetryPolicy,
}
