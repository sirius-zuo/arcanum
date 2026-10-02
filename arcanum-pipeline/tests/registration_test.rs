use arcanum_core::traits::{
    ChunkMetadataStore, Chunker, Embedder, InMemoryChunkMetadataStore, Preprocessor, Source,
    VectorStore,
};
use arcanum_core::types::*;
use arcanum_core::ArcanumError;
use arcanum_ingestion::{FixedSizeChunker, LoaderRegistry, RawLoader};
use arcanum_pipeline::{ArcanumPipelineRegistry, DagExecutor, IngestionState, PipelineDeps};
use async_trait::async_trait;
use std::sync::{Arc, Mutex as StdMutex};
use tokio::sync::Mutex;

struct CapturingStore {
    captured: StdMutex<Vec<Chunk>>,
    fail: bool,
}

#[async_trait]
impl VectorStore for CapturingStore {
    async fn upsert(&self, _: &str, chunks: Vec<IndexedChunk>) -> arcanum_core::Result<()> {
        if self.fail {
            return Err(ArcanumError::Storage("upsert failed".into()));
        }
        self.captured
            .lock()
            .unwrap()
            .extend(chunks.into_iter().map(|c| c.chunk));
        Ok(())
    }
    async fn search(
        &self,
        _: &str,
        _: &arcanum_core::traits::VectorQuery,
    ) -> arcanum_core::Result<Vec<arcanum_core::traits::ScoredChunk>> {
        Ok(vec![])
    }
    async fn delete(&self, _: &str, _: &[ChunkId]) -> arcanum_core::Result<()> {
        Ok(())
    }
    async fn collection_exists(&self, _: &str) -> arcanum_core::Result<bool> {
        Ok(true)
    }
    async fn delete_by_source_uri(&self, _: &str, _: &str) -> arcanum_core::Result<()> {
        Ok(())
    }
}

struct PassThrough;
#[async_trait]
impl Preprocessor for PassThrough {
    async fn process(&self, doc: RawDocument) -> arcanum_core::Result<RawDocument> {
        Ok(doc)
    }
}

struct OneVectorEmbedder;
#[async_trait]
impl Embedder for OneVectorEmbedder {
    async fn embed(&self, t: Vec<String>) -> arcanum_core::Result<Vec<Vector>> {
        Ok(t.iter().map(|_| Vector(vec![0.0, 1.0, 0.0])).collect())
    }
    fn dimension(&self) -> usize {
        3
    }
}

struct FailingRegistry;
#[async_trait]
impl ChunkMetadataStore for FailingRegistry {
    async fn put(&self, _: &ChunkMetadataRecord) -> arcanum_core::Result<()> {
        Err(ArcanumError::Storage("registry put failed".into()))
    }
    async fn get(&self, _: &ChunkId) -> arcanum_core::Result<Option<ChunkMetadataRecord>> {
        Ok(None)
    }
    async fn get_many(&self, _: &[ChunkId]) -> arcanum_core::Result<Vec<ChunkMetadataRecord>> {
        Ok(vec![])
    }
    async fn delete_by_source_uri(&self, _: &str, _: &str) -> arcanum_core::Result<()> {
        Ok(())
    }
    async fn delete_by_document_version(
        &self,
        _: &DocumentId,
        _: u32,
    ) -> arcanum_core::Result<Vec<ChunkId>> {
        Ok(vec![])
    }
}

fn deps(
    store: Arc<CapturingStore>,
    registry: Option<Arc<dyn ChunkMetadataStore>>,
) -> Arc<PipelineDeps> {
    let chunker: Arc<dyn Chunker> = Arc::new(FixedSizeChunker::new(20, 5));
    Arc::new(PipelineDeps {
        loaders: Arc::new(LoaderRegistry::new().register(Arc::new(RawLoader::new()))),
        preprocessors: Some(Arc::new(PassThrough)),
        chunkers: PerBackendChunkers {
            vector: chunker.clone(),
            lexical: chunker.clone(),
            graph: chunker.clone(),
            tree: chunker,
        },
        context_enricher: None,
        entity_extractor: None,
        embedder: Arc::new(OneVectorEmbedder),
        vector_store: store as Arc<dyn VectorStore>,
        graph_store: None,
        tree_store: None,
        version_store: Arc::new(arcanum_core::traits::NoOpDocumentVersionStore),
        snapshot_store: Arc::new(arcanum_core::traits::InMemorySnapshotStore::new()),
        chunk_metadata: registry,
        bm25_index: None,
        cache_invalidator: Arc::new(arcanum_core::traits::CacheInvalidationBroadcaster::new(
            vec![],
        )),
        embedding_cb: Arc::new(arcanum_middleware::CircuitBreaker::new(
            "embedding",
            5,
            std::time::Duration::from_secs(30),
        )),
        vector_store_cb: Arc::new(arcanum_middleware::CircuitBreaker::new(
            "vector_store",
            5,
            std::time::Duration::from_secs(30),
        )),
        shadow: None,
    })
}

fn new_state() -> Arc<Mutex<IngestionState>> {
    Arc::new(Mutex::new(IngestionState::new(
        Source::Raw {
            content: b"Hello world. This is a test document with enough text to chunk.".to_vec(),
            mime_hint: Some("text/plain".into()),
            uri: "raw://reg-test".into(),
        },
        CollectionId("col1".into()),
    )))
}

async fn run_standard(
    deps: &Arc<PipelineDeps>,
    state: &Arc<Mutex<IngestionState>>,
) -> arcanum_core::Result<()> {
    let dag = ArcanumPipelineRegistry::default()
        .build("standard", state.clone(), deps)
        .unwrap();
    DagExecutor::execute(&dag, Default::default())
        .await
        .map(|_| ())
}

#[tokio::test]
async fn chunk_stages_stamp_snapshot_document_id() {
    let store = Arc::new(CapturingStore {
        captured: StdMutex::new(vec![]),
        fail: false,
    });
    let deps = deps(store.clone(), None);
    let state = new_state();
    run_standard(&deps, &state).await.unwrap();

    let expected = state.lock().await.snapshot_document_id.clone().unwrap();
    let captured = store.captured.lock().unwrap();
    assert!(!captured.is_empty());
    for c in captured.iter() {
        assert_eq!(c.document_id, expected);
        assert_eq!(c.provenance.document_version, 1);
    }
}

#[tokio::test]
async fn vector_write_failure_leaves_no_registry_rows() {
    let store = Arc::new(CapturingStore {
        captured: StdMutex::new(vec![]),
        fail: true,
    });
    let registry = Arc::new(InMemoryChunkMetadataStore::new());
    let deps = deps(store, Some(registry.clone() as Arc<dyn ChunkMetadataStore>));
    let state = new_state();
    assert!(run_standard(&deps, &state).await.is_err());
    assert!(registry.get_all().await.is_empty());
}

#[tokio::test]
async fn registry_write_failure_fails_vector_write() {
    let store = Arc::new(CapturingStore {
        captured: StdMutex::new(vec![]),
        fail: false,
    });
    let deps = deps(store, Some(Arc::new(FailingRegistry)));
    let state = new_state();
    assert!(run_standard(&deps, &state).await.is_err());
}

// ---- graph and tree lines register their own chunks ----

struct OneEntityEnricher;
#[async_trait]
impl arcanum_core::traits::TextEnricher for OneEntityEnricher {
    async fn enrich(&self, req: EnrichRequest) -> arcanum_core::Result<EnrichedText> {
        match req.intent {
            EnrichIntent::ExtractEntities => Ok(EnrichedText(
                serde_json::json!({
                    "entities": [{"name": "Thing", "entity_type": "ORG"}],
                    "relations": []
                })
                .to_string(),
            )),
            _ => Ok(EnrichedText("summary".into())),
        }
    }
}

struct CapturingGraph {
    entities: StdMutex<Vec<Entity>>,
    fail: bool,
}
#[async_trait]
impl arcanum_core::traits::GraphStore for CapturingGraph {
    async fn upsert_entities(&self, _: &str, e: Vec<Entity>) -> arcanum_core::Result<()> {
        if self.fail {
            return Err(ArcanumError::Storage("graph write failed".into()));
        }
        self.entities.lock().unwrap().extend(e);
        Ok(())
    }
    async fn upsert_relations(&self, _: &str, _: Vec<Relation>) -> arcanum_core::Result<()> {
        Ok(())
    }
    async fn query(
        &self,
        _: &str,
        _: &arcanum_core::traits::GraphQuery,
    ) -> arcanum_core::Result<Vec<Entity>> {
        Ok(vec![])
    }
    async fn get_relations(&self, _: &EntityId) -> arcanum_core::Result<Vec<Relation>> {
        Ok(vec![])
    }
    async fn delete_by_source_uri(&self, _: &str, _: &str) -> arcanum_core::Result<()> {
        Ok(())
    }
}

struct EmptyChunker;
#[async_trait]
impl Chunker for EmptyChunker {
    async fn chunk(&self, _: &RawDocument) -> arcanum_core::Result<Vec<Chunk>> {
        Ok(vec![])
    }
}

/// Full-template deps with a graph store, tree store and registry; `tree_chunker` overrides
/// the tree line's chunker.
fn full_deps(
    graph: Arc<CapturingGraph>,
    tree: Arc<arcanum_tree::InMemoryTreeStore>,
    registry: Arc<InMemoryChunkMetadataStore>,
    tree_chunker: Option<Arc<dyn Chunker>>,
) -> Arc<PipelineDeps> {
    let store = Arc::new(CapturingStore {
        captured: StdMutex::new(vec![]),
        fail: false,
    });
    let mut d = Arc::try_unwrap(deps(store, Some(registry as Arc<dyn ChunkMetadataStore>)))
        .ok()
        .unwrap();
    d.entity_extractor = Some(Arc::new(OneEntityEnricher));
    d.context_enricher = Some(Arc::new(OneEntityEnricher));
    d.graph_store = Some(graph);
    d.tree_store = Some(tree);
    if let Some(c) = tree_chunker {
        d.chunkers.tree = c;
    }
    Arc::new(d)
}

async fn run_full(
    deps: &Arc<PipelineDeps>,
    state: &Arc<Mutex<IngestionState>>,
) -> arcanum_core::Result<()> {
    let dag = ArcanumPipelineRegistry::default()
        .build("full", state.clone(), deps)
        .unwrap();
    DagExecutor::execute(&dag, Default::default())
        .await
        .map(|_| ())
}

async fn leaf_ids(tree: &arcanum_tree::InMemoryTreeStore) -> Vec<ChunkId> {
    use arcanum_core::traits::TreeStore;
    tree.get_level("col1", 0)
        .await
        .unwrap()
        .into_iter()
        .flat_map(|n| n.leaf_chunk_ids)
        .collect()
}

#[tokio::test]
async fn graph_line_registers_graph_chunks() {
    let graph = Arc::new(CapturingGraph {
        entities: StdMutex::new(vec![]),
        fail: false,
    });
    let registry = Arc::new(InMemoryChunkMetadataStore::new());
    let deps = full_deps(
        graph.clone(),
        Arc::new(arcanum_tree::InMemoryTreeStore::new()),
        registry.clone(),
        None,
    );
    run_full(&deps, &new_state()).await.unwrap();

    let entities = graph.entities.lock().unwrap().clone();
    assert!(!entities.is_empty());
    for id in entities.iter().flat_map(|e| e.source_chunks.iter()) {
        let row = registry.get(id).await.unwrap().expect("graph chunk row");
        assert_eq!(row.backend, ChunkBackend::Graph);
    }
}

#[tokio::test]
async fn graph_write_failure_leaves_no_graph_rows() {
    let graph = Arc::new(CapturingGraph {
        entities: StdMutex::new(vec![]),
        fail: true,
    });
    let registry = Arc::new(InMemoryChunkMetadataStore::new());
    let deps = full_deps(
        graph,
        Arc::new(arcanum_tree::InMemoryTreeStore::new()),
        registry.clone(),
        None,
    );
    let _ = run_full(&deps, &new_state()).await;
    assert!(registry
        .get_all()
        .await
        .iter()
        .all(|r| r.backend != ChunkBackend::Graph));
}

#[tokio::test]
async fn tree_line_registers_tree_chunks() {
    let tree = Arc::new(arcanum_tree::InMemoryTreeStore::new());
    let registry = Arc::new(InMemoryChunkMetadataStore::new());
    let deps = full_deps(
        Arc::new(CapturingGraph {
            entities: StdMutex::new(vec![]),
            fail: false,
        }),
        tree.clone(),
        registry.clone(),
        None,
    );
    run_full(&deps, &new_state()).await.unwrap();

    let ids = leaf_ids(&tree).await;
    assert!(!ids.is_empty());
    for id in ids {
        let row = registry.get(&id).await.unwrap().expect("tree chunk row");
        assert_eq!(row.backend, ChunkBackend::Tree);
    }
}

#[tokio::test]
async fn raptor_build_skips_when_tree_chunks_empty() {
    let tree = Arc::new(arcanum_tree::InMemoryTreeStore::new());
    let registry = Arc::new(InMemoryChunkMetadataStore::new());
    let deps = full_deps(
        Arc::new(CapturingGraph {
            entities: StdMutex::new(vec![]),
            fail: false,
        }),
        tree.clone(),
        registry.clone(),
        Some(Arc::new(EmptyChunker)),
    );
    run_full(&deps, &new_state()).await.unwrap();

    assert!(leaf_ids(&tree).await.is_empty());
    assert!(registry
        .get_all()
        .await
        .iter()
        .all(|r| r.backend != ChunkBackend::Tree));
}
