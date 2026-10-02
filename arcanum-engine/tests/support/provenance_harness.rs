//! Shared harness for provenance-integrity tests: deterministic embedder,
//! cosine vector store, scripted enricher, and a full-template ingestion
//! fixture over two documents.
//!
//! Extension point: when a lexical chunker exists on `PerBackendChunkers`
//! (plan Task 5), add `lexical: fixed{80,0}` in `ingest_fixture`.
#![allow(dead_code)]

use arcanum_core::traits::{
    CacheInvalidationBroadcaster, ChunkMetadataStore, DocumentVersionStore, Embedder, GraphStore,
    InMemoryChunkMetadataStore, InMemorySnapshotStore, Preprocessor, ScoredChunk, Source,
    TextEnricher, TreeStore, VectorQuery, VectorStore,
};
use arcanum_core::types::{
    ChunkId, ChunkStrategyConfig, CollectionId, DocumentId, EnrichIntent, EnrichRequest,
    EnrichedText, FilterOp, IndexedChunk, PerBackendChunkers, RawDocument, Vector,
};
use arcanum_core::Result;
use arcanum_graph::InMemoryGraphStore;
use arcanum_ingestion::{default_registry, LoaderRegistry, RawLoader, SqliteDocumentVersionStore};
use arcanum_pipeline::{ArcanumPipelineRegistry, DagExecutor, IngestionState, PipelineDeps};
use arcanum_tree::InMemoryTreeStore;
use arcanum_vector::Bm25Index;
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::{Arc, Mutex as StdMutex};
use tokio::sync::Mutex;

pub const COLLECTION: &str = "prov";
pub const URI_A: &str = "raw://doc-a";
pub const URI_B: &str = "raw://doc-b";

pub const DOC_A: &str = "Acme Corp builds rockets for orbital delivery. The company was founded \
by engineers who loved the cafe culture of Paris, and every morning the team gathers over a \
caf\u{e9} to review telemetry. Bob works at Acme Corp as the lead propulsion engineer, and his \
na\u{ef}ve first prototype exploded on the pad. Since then the rockets have flown forty times \
without incident. Acme Corp ships payloads for research labs, telecom operators, and small \
satellite startups across three continents. Bob mentors new hires on safe engine testing.";

pub const DOC_B: &str = "Gardening in early spring starts with preparing the soil. Turn the beds \
over, mix in compost, and wait for the frost to pass before planting tomatoes. Tomatoes need full \
sun and steady watering. Herbs such as basil and parsley grow well in pots on a sunny window sill. \
Mulch the beds to keep weeds down and moisture in. Prune roses after the last hard frost, and \
water deeply once a week instead of a little every day. Compost tea gives the vegetables a gentle \
boost during the growing season.";

fn fnv1a(token: &str) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in token.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

/// 64-dim bag-of-words hashing embedder, L2-normalised.
pub struct KeywordEmbedder;

impl KeywordEmbedder {
    pub fn embed_one(text: &str) -> Vector {
        let mut v = vec![0.0f32; 64];
        for tok in text
            .to_lowercase()
            .split(|c: char| !c.is_alphanumeric())
            .filter(|t| !t.is_empty())
        {
            v[(fnv1a(tok) % 64) as usize] += 1.0;
        }
        let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            for x in v.iter_mut() {
                *x /= norm;
            }
        }
        Vector(v)
    }
}

#[async_trait]
impl Embedder for KeywordEmbedder {
    async fn embed(&self, texts: Vec<String>) -> Result<Vec<Vector>> {
        Ok(texts.iter().map(|t| Self::embed_one(t)).collect())
    }
    fn dimension(&self) -> usize {
        64
    }
}

fn cosine(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let na = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let nb = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if na == 0.0 || nb == 0.0 {
        0.0
    } else {
        dot / (na * nb)
    }
}

/// In-memory cosine top-k store. Honors only `chunk_id IN [...]` filters.
#[derive(Default)]
pub struct TestVectorStore {
    data: StdMutex<HashMap<String, Vec<IndexedChunk>>>,
}

#[async_trait]
impl VectorStore for TestVectorStore {
    async fn upsert(&self, collection: &str, chunks: Vec<IndexedChunk>) -> Result<()> {
        let mut g = self.data.lock().unwrap();
        let col = g.entry(collection.to_string()).or_default();
        for c in chunks {
            col.retain(|e| e.chunk.id.0 != c.chunk.id.0);
            col.push(c);
        }
        Ok(())
    }

    async fn search(&self, collection: &str, query: &VectorQuery) -> Result<Vec<ScoredChunk>> {
        let allowed: Option<Vec<String>> = query
            .filters
            .iter()
            .find(|f| f.field == "chunk_id" && matches!(f.op, FilterOp::In))
            .and_then(|f| f.value.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            });
        let g = self.data.lock().unwrap();
        let mut scored: Vec<ScoredChunk> = g
            .get(collection)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
            .iter()
            .filter(|c| {
                allowed
                    .as_ref()
                    .map(|ids| ids.contains(&c.chunk.id.0.to_string()))
                    .unwrap_or(true)
            })
            .map(|c| ScoredChunk {
                score: cosine(&query.vector.0, &c.vector.0),
                chunk: c.clone(),
            })
            .collect();
        scored.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
        scored.truncate(query.top_k);
        Ok(scored)
    }

    async fn delete(&self, collection: &str, ids: &[ChunkId]) -> Result<()> {
        if let Some(col) = self.data.lock().unwrap().get_mut(collection) {
            col.retain(|c| !ids.iter().any(|i| i.0 == c.chunk.id.0));
        }
        Ok(())
    }

    async fn collection_exists(&self, collection: &str) -> Result<bool> {
        Ok(self.data.lock().unwrap().contains_key(collection))
    }

    async fn delete_by_source_uri(&self, collection: &str, source_uri: &str) -> Result<()> {
        if let Some(col) = self.data.lock().unwrap().get_mut(collection) {
            col.retain(|c| c.chunk.provenance.source_uri != source_uri);
        }
        Ok(())
    }
}

/// Deterministic stand-in for an LLM enricher.
pub struct ScriptedEnricher;

#[async_trait]
impl TextEnricher for ScriptedEnricher {
    async fn enrich(&self, request: EnrichRequest) -> Result<EnrichedText> {
        let text = request.text;
        match request.intent {
            EnrichIntent::ExtractEntities => {
                let acme = text.contains("Acme");
                let bob = text.contains("Bob");
                let mut entities = vec![];
                if acme {
                    entities.push(serde_json::json!({"name":"Acme Corp","entity_type":"ORG"}));
                }
                if bob {
                    entities.push(serde_json::json!({"name":"Bob","entity_type":"PERSON"}));
                }
                let mut relations = vec![];
                if acme && bob {
                    relations.push(serde_json::json!({
                        "source":"Bob","relation":"works_at","target":"Acme Corp"
                    }));
                }
                Ok(EnrichedText(
                    serde_json::json!({"entities":entities,"relations":relations}).to_string(),
                ))
            }
            EnrichIntent::Summarize => Ok(EnrichedText(format!(
                "summary: {}",
                text.chars().take(30).collect::<String>()
            ))),
            _ => Ok(EnrichedText(text)),
        }
    }
}

struct PassThrough;

#[async_trait]
impl Preprocessor for PassThrough {
    async fn process(&self, doc: RawDocument) -> Result<RawDocument> {
        Ok(doc)
    }
}

pub struct Fixture {
    pub chunk_metadata: Arc<InMemoryChunkMetadataStore>,
    pub graph_store: Arc<InMemoryGraphStore>,
    pub tree_store: Arc<InMemoryTreeStore>,
    pub vector_store: Arc<TestVectorStore>,
    pub bm25: Arc<Bm25Index>,
    pub version_store: Arc<SqliteDocumentVersionStore>,
    pub snapshot_store: Arc<InMemorySnapshotStore>,
    pub doc_a_text: String,
    pub doc_b_text: String,
    pub doc_a_id: DocumentId,
    pub doc_b_id: DocumentId,
    // Keep the tempdir alive for the lifetime of the fixture.
    _dir: tempfile::TempDir,
}

fn fixed(chunk_size: u64, overlap: u64) -> Arc<dyn arcanum_core::traits::Chunker> {
    default_registry()
        .build(&ChunkStrategyConfig {
            strategy: "fixed".into(),
            params: serde_json::json!({ "chunk_size": chunk_size, "overlap": overlap }),
        })
        .expect("fixed chunker")
}

pub async fn ingest_fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let bm25_dir = dir.path().join("bm25");
    std::fs::create_dir_all(&bm25_dir).unwrap();
    let db_path = dir.path().join("versions.db");

    let chunk_metadata = Arc::new(InMemoryChunkMetadataStore::new());
    let graph_store = Arc::new(InMemoryGraphStore::new());
    let tree_store = Arc::new(InMemoryTreeStore::new());
    let vector_store = Arc::new(TestVectorStore::default());
    let bm25 = Arc::new(Bm25Index::new(bm25_dir.to_str().unwrap()).unwrap());
    let version_store = Arc::new(
        SqliteDocumentVersionStore::open(db_path.to_str().unwrap())
            .await
            .unwrap(),
    );
    let snapshot_store = Arc::new(InMemorySnapshotStore::new());

    let enricher: Arc<dyn TextEnricher> = Arc::new(ScriptedEnricher);
    let deps = Arc::new(PipelineDeps {
        loaders: Arc::new(LoaderRegistry::new().register(Arc::new(RawLoader::new()))),
        preprocessors: Some(Arc::new(PassThrough)),
        chunkers: PerBackendChunkers {
            vector: fixed(120, 20),
            graph: fixed(200, 0),
            tree: fixed(60, 0),
        },
        shadow: None,
        context_enricher: Some(enricher.clone()),
        entity_extractor: Some(enricher),
        embedder: Arc::new(KeywordEmbedder),
        vector_store: vector_store.clone() as Arc<dyn VectorStore>,
        graph_store: Some(graph_store.clone() as Arc<dyn GraphStore>),
        tree_store: Some(tree_store.clone() as Arc<dyn TreeStore>),
        version_store: version_store.clone() as Arc<dyn DocumentVersionStore>,
        snapshot_store: snapshot_store.clone(),
        chunk_metadata: Some(chunk_metadata.clone() as Arc<dyn ChunkMetadataStore>),
        bm25_index: Some(bm25.clone()),
        cache_invalidator: Arc::new(CacheInvalidationBroadcaster::new(vec![])),
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
    });

    for (uri, text) in [(URI_A, DOC_A), (URI_B, DOC_B)] {
        let state = Arc::new(Mutex::new(IngestionState::new(
            Source::Raw {
                content: text.as_bytes().to_vec(),
                mime_hint: Some("text/plain".into()),
                uri: uri.into(),
            },
            CollectionId(COLLECTION.into()),
        )));
        let dag = ArcanumPipelineRegistry::default()
            .build("full", state, &deps)
            .unwrap();
        DagExecutor::execute(&dag, Default::default())
            .await
            .unwrap();
    }

    let doc_a_id = version_store
        .get_latest(URI_A, COLLECTION)
        .await
        .unwrap()
        .expect("doc A version")
        .document_id;
    let doc_b_id = version_store
        .get_latest(URI_B, COLLECTION)
        .await
        .unwrap()
        .expect("doc B version")
        .document_id;

    Fixture {
        chunk_metadata,
        graph_store,
        tree_store,
        vector_store,
        bm25,
        version_store,
        snapshot_store,
        doc_a_text: DOC_A.to_string(),
        doc_b_text: DOC_B.to_string(),
        doc_a_id,
        doc_b_id,
        _dir: dir,
    }
}
