use arcanum_pipeline::{ArcanumPipelineRegistry, PipelineDeps, IngestionState};
use arcanum_core::traits::Source;
use arcanum_core::types::CollectionId;
use std::sync::Arc;
use tokio::sync::Mutex;

fn stub_deps() -> Arc<PipelineDeps> {
    use arcanum_ingestion::{LoaderRegistry, PreprocessorRegistry, DocumentHashTracker, RawLoader};
    use arcanum_core::traits::{Chunker, Embedder, VectorStore};
    use arcanum_core::types::*;
    use async_trait::async_trait;

    struct StubChunker;
    #[async_trait]
    impl Chunker for StubChunker {
        async fn chunk(&self, _doc: &RawDocument) -> arcanum_core::Result<Vec<Chunk>> { Ok(vec![]) }
    }
    struct StubEmbedder;
    #[async_trait]
    impl Embedder for StubEmbedder {
        async fn embed(&self, _t: Vec<String>) -> arcanum_core::Result<Vec<Vector>> { Ok(vec![]) }
        fn dimension(&self) -> usize { 3 }
    }
    struct StubVectorStore;
    #[async_trait]
    impl VectorStore for StubVectorStore {
        async fn upsert(&self, _: &str, _: Vec<IndexedChunk>) -> arcanum_core::Result<()> { Ok(()) }
        async fn search(&self, _: &str, _: &arcanum_core::traits::VectorQuery) -> arcanum_core::Result<Vec<arcanum_core::traits::ScoredChunk>> { Ok(vec![]) }
        async fn delete(&self, _: &str, _: &[ChunkId]) -> arcanum_core::Result<()> { Ok(()) }
        async fn collection_exists(&self, _: &str) -> arcanum_core::Result<bool> { Ok(true) }
    }

    Arc::new(PipelineDeps {
        loaders: Arc::new(LoaderRegistry::new().register(Arc::new(RawLoader::new()))),
        preprocessors: Arc::new(PreprocessorRegistry::new()),
        chunker: Arc::new(StubChunker),
        context_enricher: None,
        entity_extractor: None,
        embedder: Arc::new(StubEmbedder),
        vector_store: Arc::new(StubVectorStore),
        graph_store: None,
        tree_store: None,
        hash_tracker: Arc::new(DocumentHashTracker::new()),
    })
}

#[test]
fn test_registry_default_contains_standard() {
    let reg = ArcanumPipelineRegistry::default();
    let state = Arc::new(Mutex::new(IngestionState::new(
        Source::File("/tmp/x".into()), CollectionId("col".into()),
    )));
    let deps = stub_deps();
    assert!(reg.build("standard", state, &deps).is_ok());
}

#[test]
fn test_registry_unknown_template_errors() {
    let reg = ArcanumPipelineRegistry::default();
    let state = Arc::new(Mutex::new(IngestionState::new(
        Source::File("/tmp/x".into()), CollectionId("col".into()),
    )));
    let deps = stub_deps();
    assert!(reg.build("nonexistent", state, &deps).is_err());
}

#[test]
fn test_registry_all_five_templates_build() {
    let reg = ArcanumPipelineRegistry::default();
    let deps = stub_deps();
    for name in ["standard", "contextual", "graph", "raptor", "full"] {
        let state = Arc::new(Mutex::new(IngestionState::new(
            Source::File("/tmp/x".into()), CollectionId("col".into()),
        )));
        assert!(reg.build(name, state, &deps).is_ok(), "template '{name}' failed to build");
    }
}
