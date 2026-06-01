//! Shared helpers for Arcanum integration example tests.
//! Each test file includes this via: `#[path = "../helpers.rs"] mod helpers;`

use arcanum_core::{traits::*, types::*, Result};
use async_trait::async_trait;
use std::{collections::HashMap, sync::Mutex};

// ── InMemoryVectorStore ────────────────────────────────────────────────────

pub struct InMemoryVectorStore {
    data: Mutex<HashMap<String, Vec<IndexedChunk>>>,
}

impl InMemoryVectorStore {
    pub fn new() -> Self {
        Self { data: Mutex::new(HashMap::new()) }
    }
}

#[async_trait]
impl VectorStore for InMemoryVectorStore {
    async fn upsert(&self, collection: &str, chunks: Vec<IndexedChunk>) -> Result<()> {
        self.data.lock().unwrap()
            .entry(collection.to_string())
            .or_default()
            .extend(chunks);
        Ok(())
    }
    async fn search(&self, collection: &str, query: &VectorQuery) -> Result<Vec<ScoredChunk>> {
        let data = self.data.lock().unwrap();
        let chunks = data.get(collection).cloned().unwrap_or_default();
        Ok(chunks.into_iter().take(query.top_k).map(|c| ScoredChunk { chunk: c, score: 0.9 }).collect())
    }
    async fn delete(&self, _collection: &str, _ids: &[ChunkId]) -> Result<()> { Ok(()) }
    async fn collection_exists(&self, collection: &str) -> Result<bool> {
        Ok(self.data.lock().unwrap().contains_key(collection))
    }
}

// ── FixedEmbedder ─────────────────────────────────────────────────────────

pub struct FixedEmbedder {
    pub dim: usize,
}

impl FixedEmbedder {
    pub fn new(dim: usize) -> Self { Self { dim } }
}

#[async_trait]
impl Embedder for FixedEmbedder {
    async fn embed(&self, texts: Vec<String>) -> Result<Vec<Vector>> {
        Ok(texts.iter().enumerate().map(|(i, _)| {
            let mut v = vec![0.1f32; self.dim];
            if self.dim > 0 { v[i % self.dim] = 1.0; }
            Vector(v)
        }).collect())
    }
    fn dimension(&self) -> usize { self.dim }
}

// ── JsonEntityEnricher ────────────────────────────────────────────────────
//
// For graph-routing tests: wraps the first few alphabetic words from the
// query as JSON entities so GraphQueryPlanner extracts them without an LLM.

pub struct JsonEntityEnricher;

#[async_trait]
impl TextEnricher for JsonEntityEnricher {
    async fn enrich(&self, req: EnrichRequest) -> Result<EnrichedText> {
        match req.intent {
            EnrichIntent::ExtractEntities => {
                let entities: Vec<String> = req.text
                    .split_whitespace()
                    .filter(|w| w.len() >= 3 && w.chars().next().map(|c| c.is_alphabetic()).unwrap_or(false))
                    .take(4)
                    .map(|w| {
                        let clean: String = w.chars().filter(|c| c.is_alphanumeric() || *c == ' ').collect();
                        format!(r#"{{"name":"{}","type":"Unknown"}}"#, clean)
                    })
                    .collect();
                Ok(EnrichedText(format!(
                    r#"{{"entities":[{}],"relations":[]}}"#,
                    entities.join(",")
                )))
            }
            _ => Ok(EnrichedText(req.text)),
        }
    }
}

// ── InMemoryLexicalIndex ──────────────────────────────────────────────────

pub struct InMemoryLexicalIndex {
    docs: Mutex<HashMap<String, Vec<(String, String)>>>,
}

impl InMemoryLexicalIndex {
    pub fn new() -> Self { Self { docs: Mutex::new(HashMap::new()) } }

    pub fn add(&self, collection: &str, id: &str, text: &str) {
        self.docs.lock().unwrap()
            .entry(collection.to_string())
            .or_default()
            .push((id.to_string(), text.to_string()));
    }
}

#[async_trait]
impl LexicalIndex for InMemoryLexicalIndex {
    async fn search(&self, collection_id: &str, query: &str, top_k: usize) -> Result<Vec<(String, f32)>> {
        let docs = self.docs.lock().unwrap();
        let entries = docs.get(collection_id).cloned().unwrap_or_default();
        let query_lower = query.to_lowercase();
        let mut hits: Vec<(String, f32)> = entries.iter()
            .filter_map(|(id, text)| {
                let score = query_lower.split_whitespace()
                    .filter(|w| text.to_lowercase().contains(*w))
                    .count() as f32;
                if score > 0.0 { Some((id.clone(), score)) } else { None }
            })
            .collect();
        hits.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        hits.truncate(top_k);
        Ok(hits)
    }
}

// ── Seed helpers ─────────────────────────────────────────────────────────

pub fn make_indexed_chunk(collection: &str, text: &str, dim: usize) -> IndexedChunk {
    IndexedChunk {
        chunk: Chunk {
            id: ChunkId::new(),
            text: text.to_string(),
            document_id: DocumentId::new(),
            collection_id: CollectionId(collection.to_string()),
            position: ChunkPosition { start: 0, end: text.len(), index: 0 },
            metadata: ChunkMetadata::default(),
        },
        vector: Vector(vec![0.1f32; dim]),
        token_vectors: None,
        store_id: uuid::Uuid::new_v4().to_string(),
    }
}

/// Seeds the vector store and returns the seeded chunks (so callers can link chunk IDs to graph entities).
pub async fn seed_vector(
    store: &InMemoryVectorStore,
    collection: &str,
    texts: &[&str],
    dim: usize,
) -> Vec<IndexedChunk> {
    let chunks: Vec<IndexedChunk> = texts.iter().map(|t| make_indexed_chunk(collection, t, dim)).collect();
    store.upsert(collection, chunks.clone()).await.unwrap();
    chunks
}

pub fn seed_lexical(index: &InMemoryLexicalIndex, collection: &str, texts: &[&str]) {
    for (i, text) in texts.iter().enumerate() {
        index.add(collection, &format!("doc-{}", i), text);
    }
}
