use arcanum_core::traits::{
    ChunkMetadataStore, Chunker, Embedder, InMemoryChunkMetadataStore, Preprocessor, Source,
    VectorStore,
};
use arcanum_core::types::*;
use arcanum_ingestion::{FixedSizeChunker, LoaderRegistry, RawLoader};
use arcanum_pipeline::{
    dag::CTX_REPLACE, ArcanumPipelineRegistry, DagExecutor, IngestionState, PipelineDeps,
};
use arcanum_vector::Bm25Index;
use async_trait::async_trait;
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::Mutex;

const COLLECTION: &str = "col1";
const URI: &str = "raw://lexical-test";

struct NullStore;
#[async_trait]
impl VectorStore for NullStore {
    async fn upsert(&self, _: &str, _: Vec<IndexedChunk>) -> arcanum_core::Result<()> {
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

fn deps(
    bm25: Option<Arc<Bm25Index>>,
    registry: Arc<InMemoryChunkMetadataStore>,
) -> Arc<PipelineDeps> {
    let vector: Arc<dyn Chunker> = Arc::new(FixedSizeChunker::new(20, 5));
    let lexical: Arc<dyn Chunker> = Arc::new(FixedSizeChunker::new(30, 0));
    Arc::new(PipelineDeps {
        loaders: Arc::new(LoaderRegistry::new().register(Arc::new(RawLoader::new()))),
        preprocessors: Some(Arc::new(PassThrough)),
        chunkers: PerBackendChunkers {
            vector: vector.clone(),
            lexical,
            graph: vector.clone(),
            tree: vector,
        },
        context_enricher: None,
        entity_extractor: None,
        embedder: Arc::new(OneVectorEmbedder),
        vector_store: Arc::new(NullStore),
        graph_store: None,
        tree_store: None,
        version_store: Arc::new(arcanum_core::traits::NoOpDocumentVersionStore),
        snapshot_store: Arc::new(arcanum_core::traits::InMemorySnapshotStore::new()),
        chunk_metadata: Some(registry as Arc<dyn ChunkMetadataStore>),
        bm25_index: bm25,
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

fn state(text: &str) -> Arc<Mutex<IngestionState>> {
    Arc::new(Mutex::new(IngestionState::new(
        Source::Raw {
            content: text.as_bytes().to_vec(),
            mime_hint: Some("text/plain".into()),
            uri: URI.into(),
        },
        CollectionId(COLLECTION.into()),
    )))
}

async fn ingest(
    template: &str,
    deps: &PipelineDeps,
    text: &str,
    replace: bool,
) -> arcanum_core::Result<()> {
    let dag = ArcanumPipelineRegistry::default()
        .build(template, state(text), deps)
        .unwrap();
    let mut ctx = arcanum_pipeline::StageContext::default();
    if replace {
        ctx.insert(CTX_REPLACE.to_string(), serde_json::json!(true));
    }
    DagExecutor::execute(&dag, ctx).await.map(|_| ())
}

const TEXT: &str = "Alpha bravo charlie delta echo foxtrot golf hotel india juliet kilo lima mike";

#[tokio::test]
async fn lexical_line_indexes_and_registers_own_chunks() {
    let dir = tempfile::tempdir().unwrap();
    let bm25 = Arc::new(Bm25Index::new(dir.path().to_str().unwrap()).unwrap());
    let registry = Arc::new(InMemoryChunkMetadataStore::new());
    let deps = deps(Some(bm25.clone()), registry.clone());
    ingest("standard", &deps, TEXT, false).await.unwrap();

    let rows = registry.get_all().await;
    let lexical: HashSet<ChunkId> = rows
        .iter()
        .filter(|r| r.backend == ChunkBackend::Lexical)
        .map(|r| r.chunk_id.clone())
        .collect();
    let vector: HashSet<ChunkId> = rows
        .iter()
        .filter(|r| r.backend == ChunkBackend::Vector)
        .map(|r| r.chunk_id.clone())
        .collect();
    assert!(!lexical.is_empty() && !vector.is_empty());
    assert!(lexical.is_disjoint(&vector));

    let hits: HashSet<ChunkId> = bm25
        .search(
            COLLECTION,
            "alpha bravo charlie delta echo foxtrot golf hotel india juliet kilo lima mike",
            50,
        )
        .unwrap()
        .into_iter()
        .map(|(id, _)| id)
        .collect();
    assert_eq!(hits, lexical, "BM25 ids must equal Lexical registry ids");
}

#[tokio::test]
async fn lexical_write_failure_is_stage_failure() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let bm25 = Arc::new(Bm25Index::new(dir.path().to_str().unwrap()).unwrap());
    let registry = Arc::new(InMemoryChunkMetadataStore::new());
    let deps = deps(Some(bm25), registry.clone());
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o500)).unwrap();
    let result = ingest("standard", &deps, TEXT, false).await;
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();

    assert!(result.is_err(), "lexical write failure must fail the run");
    assert!(registry
        .get_all()
        .await
        .iter()
        .all(|r| r.backend != ChunkBackend::Lexical));
}

#[tokio::test]
async fn templates_include_lexical_stages_iff_bm25_configured() {
    let dir = tempfile::tempdir().unwrap();
    let bm25 = Arc::new(Bm25Index::new(dir.path().to_str().unwrap()).unwrap());
    let registry = Arc::new(InMemoryChunkMetadataStore::new());
    let with = deps(Some(bm25), registry.clone());
    let without = deps(None, registry);
    for template in ["standard", "contextual", "graph", "raptor", "full"] {
        for (d, expected) in [(&with, true), (&without, false)] {
            let dag = ArcanumPipelineRegistry::default()
                .build(template, state(TEXT), d)
                .unwrap();
            let ids: Vec<&str> = dag.stages.iter().map(|s| s.id).collect();
            assert_eq!(ids.contains(&"lexical_chunk"), expected, "{template}");
            assert_eq!(ids.contains(&"lexical_write"), expected, "{template}");
        }
    }
}

#[tokio::test]
async fn reingest_replaces_lexical_entries() {
    let dir = tempfile::tempdir().unwrap();
    let bm25 = Arc::new(Bm25Index::new(dir.path().to_str().unwrap()).unwrap());
    let registry = Arc::new(InMemoryChunkMetadataStore::new());
    let deps = deps(Some(bm25.clone()), registry.clone());

    ingest("standard", &deps, "zebra quagga okapi gnu eland", false)
        .await
        .unwrap();
    ingest("standard", &deps, "walrus narwhal beluga orca", true)
        .await
        .unwrap();

    assert!(bm25.search(COLLECTION, "zebra", 10).unwrap().is_empty());
    let v2: HashSet<ChunkId> = bm25
        .search(COLLECTION, "walrus", 10)
        .unwrap()
        .into_iter()
        .map(|(id, _)| id)
        .collect();
    let lexical: HashSet<ChunkId> = registry
        .get_all()
        .await
        .into_iter()
        .filter(|r| r.backend == ChunkBackend::Lexical)
        .map(|r| r.chunk_id)
        .collect();
    assert!(!v2.is_empty());
    assert!(v2.is_subset(&lexical));
}
