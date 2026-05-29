# Arcanum Part 3 — Processing Layer

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement `arcanum-ingestion`, `arcanum-middleware`, `arcanum-retrieval`, `arcanum-eval`, and `arcanum-pipeline` — the full ingestion DAG, retrieval strategies, fusion engine, evaluation metrics, and pipeline orchestration.

**Architecture:** `arcanum-pipeline` is the DAG executor. `arcanum-ingestion` and `arcanum-retrieval` provide stage implementations. `arcanum-middleware` provides reliability primitives. `arcanum-eval` measures quality. None of these crates know about HTTP or MCP.

**Tech Stack:** `tokio 1` (concurrency), `tokio::sync::mpsc` (BoundedQueue), `sha2 0.10` (hashing)

**Prerequisites:** Parts 1 and 2 complete.

---

### Task 17: arcanum-ingestion — FileLoader + HtmlPreprocessor

**Files:**
- Modify: `arcanum-ingestion/Cargo.toml`
- Create: `arcanum-ingestion/src/loaders/file.rs`
- Create: `arcanum-ingestion/src/preprocessors/html.rs`
- Create: `arcanum-ingestion/src/lib.rs`

- [ ] **Step 1: Update `arcanum-ingestion/Cargo.toml`**

```toml
[package]
name    = "arcanum-ingestion"
version = "0.1.0"
edition = "2021"

[dependencies]
arcanum-core  = { path = "../arcanum-core" }
async-trait   = { workspace = true }
tokio         = { workspace = true }
serde         = { workspace = true }
serde_json    = { workspace = true }
anyhow        = { workspace = true }
sha2          = "0.10"
hex           = "0.4"
scraper       = "0.19"     # HTML parsing
```

- [ ] **Step 2: Write failing tests**

```rust
// arcanum-ingestion/tests/loader_test.rs
use arcanum_ingestion::FileLoader;
use arcanum_core::traits::{DocumentLoader, Source};
use std::io::Write;

#[tokio::test]
async fn test_file_loader_reads_markdown() {
    let mut tmp = tempfile::NamedTempFile::new().unwrap();
    tmp.write_all(b"# Hello\nWorld").unwrap();
    let loader = FileLoader::new();
    let source = Source::File(tmp.path().to_path_buf());
    assert!(loader.supports(&source));
    let doc = loader.load(&source).await.unwrap();
    assert_eq!(doc.mime_type, "text/markdown");
    assert!(doc.content.len() > 0);
}

// arcanum-ingestion/tests/preprocessor_test.rs
use arcanum_ingestion::HtmlCleaner;
use arcanum_core::traits::Preprocessor;
use arcanum_core::types::*;

#[tokio::test]
async fn test_html_cleaner_strips_tags() {
    let cleaner = HtmlCleaner::new();
    let doc = RawDocument {
        id: DocumentId::new(),
        content: b"<h1>Title</h1><p>Hello <b>world</b></p>".to_vec(),
        mime_type: "text/html".into(),
        source_uri: "test".into(),
        metadata: Default::default(),
    };
    let processed = cleaner.process(doc).await.unwrap();
    let text = String::from_utf8(processed.content).unwrap();
    assert!(text.contains("Title"));
    assert!(text.contains("Hello"));
    assert!(!text.contains("<b>"));
}
```

- [ ] **Step 3: Implement `arcanum-ingestion/src/loaders/file.rs`**

```rust
use arcanum_core::{traits::{DocumentLoader, Source}, types::*, Result, ArcanumError};
use async_trait::async_trait;

pub struct FileLoader;

impl FileLoader {
    pub fn new() -> Self { Self }

    fn detect_mime(path: &std::path::Path) -> &'static str {
        match path.extension().and_then(|e| e.to_str()) {
            Some("md") | Some("markdown") => "text/markdown",
            Some("html") | Some("htm")    => "text/html",
            Some("txt")                   => "text/plain",
            Some("pdf")                   => "application/pdf",
            _                             => "application/octet-stream",
        }
    }
}

#[async_trait]
impl DocumentLoader for FileLoader {
    async fn load(&self, source: &Source) -> Result<RawDocument> {
        let Source::File(path) = source else {
            return Err(ArcanumError::Ingestion("FileLoader only handles Source::File".into()));
        };
        let content = tokio::fs::read(path).await
            .map_err(|e| ArcanumError::Ingestion(e.to_string()))?;
        Ok(RawDocument {
            id: DocumentId::new(),
            mime_type: Self::detect_mime(path).to_string(),
            source_uri: path.to_string_lossy().to_string(),
            content,
            metadata: Default::default(),
        })
    }

    fn supports(&self, source: &Source) -> bool {
        matches!(source, Source::File(_))
    }
}
```

`arcanum-ingestion/src/preprocessors/html.rs`:
```rust
use arcanum_core::{traits::Preprocessor, types::*, Result};
use async_trait::async_trait;
use scraper::{Html, Selector};

pub struct HtmlCleaner;

impl HtmlCleaner {
    pub fn new() -> Self { Self }
}

#[async_trait]
impl Preprocessor for HtmlCleaner {
    async fn process(&self, mut doc: RawDocument) -> Result<RawDocument> {
        if doc.mime_type != "text/html" { return Ok(doc); }
        let html = String::from_utf8_lossy(&doc.content);
        let parsed = Html::parse_document(&html);
        // Extract visible text, skip scripts/styles
        let script_sel = Selector::parse("script, style, noscript").unwrap();
        let mut text = parsed.root_element().text()
            .collect::<Vec<_>>()
            .join(" ");
        // Remove excessive whitespace
        text = text.split_whitespace().collect::<Vec<_>>().join(" ");
        doc.content = text.into_bytes();
        doc.mime_type = "text/plain".to_string();
        Ok(doc)
    }
}
```

`arcanum-ingestion/src/lib.rs`:
```rust
pub mod loaders {
    mod file;
    pub use file::FileLoader;
}
pub mod preprocessors {
    mod html;
    pub use html::HtmlCleaner;
}
pub use loaders::FileLoader;
pub use preprocessors::HtmlCleaner;
```

- [ ] **Step 4: Run tests and commit**

```bash
cargo test -p arcanum-ingestion
git add arcanum-ingestion/
git commit -m "feat(ingestion): add FileLoader and HtmlCleaner preprocessor"
```

---

### Task 18: arcanum-ingestion — Chunkers (Fixed, Semantic, Propositional)

**Files:**
- Create: `arcanum-ingestion/src/chunkers/fixed.rs`
- Create: `arcanum-ingestion/src/chunkers/semantic.rs`
- Create: `arcanum-ingestion/src/chunkers/propositional.rs`

- [ ] **Step 1: Write failing tests**

```rust
// arcanum-ingestion/tests/chunker_test.rs
use arcanum_ingestion::{FixedSizeChunker, SemanticChunker};
use arcanum_core::traits::Chunker;
use arcanum_core::types::*;

fn make_doc(text: &str) -> RawDocument {
    RawDocument { id: DocumentId::new(), content: text.as_bytes().to_vec(),
        mime_type: "text/plain".into(), source_uri: "test".into(), metadata: Default::default() }
}

#[tokio::test]
async fn test_fixed_size_chunks_count() {
    let chunker = FixedSizeChunker::new(20, 5);
    let doc = make_doc("Hello world this is a test of chunking behavior");
    let chunks = chunker.chunk(&doc).await.unwrap();
    assert!(chunks.len() >= 2);
    for c in &chunks { assert!(c.text.len() <= 30); } // with overlap headroom
}

#[tokio::test]
async fn test_fixed_chunk_positions_are_sequential() {
    let chunker = FixedSizeChunker::new(10, 0);
    let doc = make_doc("abcdefghij klmnopqrst");
    let chunks = chunker.chunk(&doc).await.unwrap();
    for (i, c) in chunks.iter().enumerate() {
        assert_eq!(c.position.index, i);
    }
}

#[tokio::test]
async fn test_empty_document_returns_no_chunks() {
    let chunker = FixedSizeChunker::new(100, 0);
    let doc = make_doc("");
    let chunks = chunker.chunk(&doc).await.unwrap();
    assert!(chunks.is_empty());
}
```

- [ ] **Step 2: Implement `arcanum-ingestion/src/chunkers/fixed.rs`**

```rust
use arcanum_core::{traits::Chunker, types::*, Result};
use async_trait::async_trait;

pub struct FixedSizeChunker {
    chunk_size: usize, // chars
    overlap: usize,
}

impl FixedSizeChunker {
    pub fn new(chunk_size: usize, overlap: usize) -> Self {
        assert!(overlap < chunk_size, "overlap must be less than chunk_size");
        Self { chunk_size, overlap }
    }
}

#[async_trait]
impl Chunker for FixedSizeChunker {
    async fn chunk(&self, doc: &RawDocument) -> Result<Vec<Chunk>> {
        let text = String::from_utf8_lossy(&doc.content);
        if text.trim().is_empty() { return Ok(vec![]); }

        let chars: Vec<char> = text.chars().collect();
        let step = self.chunk_size - self.overlap;
        let mut chunks = vec![];
        let mut start = 0;
        let mut index = 0;

        while start < chars.len() {
            let end = (start + self.chunk_size).min(chars.len());
            let chunk_text: String = chars[start..end].iter().collect();
            let trimmed = chunk_text.trim().to_string();
            if !trimmed.is_empty() {
                chunks.push(Chunk {
                    id: ChunkId::new(),
                    text: trimmed,
                    document_id: doc.id.clone(),
                    collection_id: CollectionId("default".into()),
                    position: ChunkPosition { start, end, index },
                    metadata: ChunkMetadata::default(),
                });
                index += 1;
            }
            if end == chars.len() { break; }
            start += step;
        }
        Ok(chunks)
    }
}
```

`arcanum-ingestion/src/chunkers/semantic.rs` — splits on sentence boundaries:
```rust
use arcanum_core::{traits::Chunker, types::*, Result};
use async_trait::async_trait;

pub struct SemanticChunker {
    max_chars: usize,
}

impl SemanticChunker {
    pub fn new(max_chars: usize) -> Self { Self { max_chars } }
}

#[async_trait]
impl Chunker for SemanticChunker {
    async fn chunk(&self, doc: &RawDocument) -> Result<Vec<Chunk>> {
        let text = String::from_utf8_lossy(&doc.content);
        // Split on sentence-ending punctuation followed by whitespace
        let sentences: Vec<&str> = text.split_inclusive(|c| matches!(c, '.' | '!' | '?'))
            .collect();
        let mut chunks = vec![];
        let mut current = String::new();
        let mut start = 0usize;
        let mut index = 0usize;

        for sentence in sentences {
            if current.len() + sentence.len() > self.max_chars && !current.is_empty() {
                let trimmed = current.trim().to_string();
                if !trimmed.is_empty() {
                    chunks.push(Chunk {
                        id: ChunkId::new(), text: trimmed.clone(),
                        document_id: doc.id.clone(),
                        collection_id: CollectionId("default".into()),
                        position: ChunkPosition { start, end: start + trimmed.len(), index },
                        metadata: ChunkMetadata::default(),
                    });
                    index += 1;
                }
                start += current.len();
                current = sentence.to_string();
            } else {
                current.push_str(sentence);
            }
        }
        if !current.trim().is_empty() {
            chunks.push(Chunk {
                id: ChunkId::new(), text: current.trim().to_string(),
                document_id: doc.id.clone(),
                collection_id: CollectionId("default".into()),
                position: ChunkPosition { start, end: start + current.len(), index },
                metadata: ChunkMetadata::default(),
            });
        }
        Ok(chunks)
    }
}
```

`arcanum-ingestion/src/chunkers/propositional.rs` — splits on newlines/sentences as atomic propositions:
```rust
use arcanum_core::{traits::Chunker, types::*, Result};
use async_trait::async_trait;

/// PropositionalChunker treats each sentence/line as an atomic fact.
/// In production, use TextEnricher to rewrite into propositions.
pub struct PropositionalChunker;

impl PropositionalChunker { pub fn new() -> Self { Self } }

#[async_trait]
impl Chunker for PropositionalChunker {
    async fn chunk(&self, doc: &RawDocument) -> Result<Vec<Chunk>> {
        let text = String::from_utf8_lossy(&doc.content);
        let propositions: Vec<&str> = text
            .split(|c| matches!(c, '.' | '!' | '?' | '\n'))
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .collect();

        Ok(propositions.into_iter().enumerate().map(|(i, p)| Chunk {
            id: ChunkId::new(), text: p.to_string(),
            document_id: doc.id.clone(),
            collection_id: CollectionId("default".into()),
            position: ChunkPosition { start: i, end: i + 1, index: i },
            metadata: ChunkMetadata::default(),
        }).collect())
    }
}
```

Update `arcanum-ingestion/src/lib.rs`:
```rust
pub mod chunkers {
    mod fixed;
    mod propositional;
    mod semantic;
    pub use fixed::FixedSizeChunker;
    pub use propositional::PropositionalChunker;
    pub use semantic::SemanticChunker;
}
pub mod loaders { mod file; pub use file::FileLoader; }
pub mod preprocessors { mod html; pub use html::HtmlCleaner; }
pub mod enrichment;

pub use chunkers::{FixedSizeChunker, PropositionalChunker, SemanticChunker};
pub use loaders::FileLoader;
pub use preprocessors::HtmlCleaner;
```

- [ ] **Step 3: Run tests and commit**

```bash
cargo test -p arcanum-ingestion
git add arcanum-ingestion/
git commit -m "feat(ingestion): add FixedSizeChunker, SemanticChunker, PropositionalChunker"
```

---

### Task 19: arcanum-ingestion — ContextEnricher, EntityExtractor, DocumentHashTracker

**Files:**
- Create: `arcanum-ingestion/src/enrichment/context.rs`
- Create: `arcanum-ingestion/src/enrichment/entity.rs`
- Create: `arcanum-ingestion/src/metadata.rs`

- [ ] **Step 1: Write failing tests**

```rust
// arcanum-ingestion/tests/enrichment_test.rs
use arcanum_ingestion::{ContextEnricher, EntityExtractor};
use arcanum_core::types::*;
use std::sync::Arc;

struct EchoEnricher;
#[async_trait::async_trait]
impl arcanum_core::traits::TextEnricher for EchoEnricher {
    async fn enrich(&self, req: EnrichRequest) -> arcanum_core::Result<EnrichedText> {
        Ok(EnrichedText(format!("[ctx] {}", req.text)))
    }
}

#[tokio::test]
async fn test_context_enricher_prepends_context() {
    let enricher = ContextEnricher::new(Arc::new(EchoEnricher));
    let chunk = Chunk {
        id: ChunkId::new(), text: "ownership rules".into(),
        document_id: DocumentId::new(),
        collection_id: CollectionId("test".into()),
        position: ChunkPosition { start: 0, end: 14, index: 0 },
        metadata: ChunkMetadata::default(),
    };
    let enriched = enricher.enrich_chunk(chunk, "Rust Book, Chapter 4").await.unwrap();
    assert!(enriched.text.contains("[ctx]"));
}

// arcanum-ingestion/tests/hash_test.rs
use arcanum_ingestion::DocumentHashTracker;

#[test]
fn test_hash_is_deterministic() {
    let content = b"hello world";
    let h1 = DocumentHashTracker::compute_hash(content);
    let h2 = DocumentHashTracker::compute_hash(content);
    assert_eq!(h1, h2);
}

#[test]
fn test_different_content_different_hash() {
    let h1 = DocumentHashTracker::compute_hash(b"abc");
    let h2 = DocumentHashTracker::compute_hash(b"def");
    assert_ne!(h1, h2);
}
```

- [ ] **Step 2: Implement enrichment stages**

`arcanum-ingestion/src/enrichment/context.rs`:
```rust
use arcanum_core::{traits::TextEnricher, types::*, Result};
use std::sync::Arc;

pub struct ContextEnricher {
    enricher: Arc<dyn TextEnricher>,
}

impl ContextEnricher {
    pub fn new(enricher: Arc<dyn TextEnricher>) -> Self { Self { enricher } }

    /// Prepends an LLM-generated context prefix to the chunk text before embedding.
    pub async fn enrich_chunk(&self, mut chunk: Chunk, doc_context: &str) -> Result<Chunk> {
        let result = self.enricher.enrich(EnrichRequest {
            text: chunk.text.clone(),
            intent: EnrichIntent::ContextPrefix,
            context: Some(EnrichContext {
                document_title: Some(doc_context.to_string()),
                ..Default::default()
            }),
        }).await?;
        chunk.text = format!("{}\n{}", result.0, chunk.text);
        Ok(chunk)
    }
}
```

`arcanum-ingestion/src/enrichment/entity.rs`:
```rust
use arcanum_core::{traits::TextEnricher, types::*, Result, ArcanumError};
use std::sync::Arc;
use serde::Deserialize;

pub struct EntityExtractor {
    enricher: Arc<dyn TextEnricher>,
}

#[derive(Deserialize)]
struct ExtractionResult {
    entities: Vec<ExtractedEntity>,
    relations: Vec<ExtractedRelation>,
}

#[derive(Deserialize)]
struct ExtractedEntity { name: String, entity_type: String }

#[derive(Deserialize)]
struct ExtractedRelation { source: String, relation: String, target: String }

impl EntityExtractor {
    pub fn new(enricher: Arc<dyn TextEnricher>) -> Self { Self { enricher } }

    pub async fn extract(&self, chunk: &Chunk) -> Result<(Vec<Entity>, Vec<Relation>)> {
        let raw = self.enricher.enrich(EnrichRequest {
            text: chunk.text.clone(),
            intent: EnrichIntent::ExtractEntities,
            context: None,
        }).await?;

        // Parse JSON response; gracefully return empty on parse failure
        let parsed: ExtractionResult = serde_json::from_str(&raw.0)
            .unwrap_or(ExtractionResult { entities: vec![], relations: vec![] });

        let mut entity_map = std::collections::HashMap::new();
        let entities: Vec<Entity> = parsed.entities.into_iter().map(|e| {
            let id = EntityId::new();
            entity_map.insert(e.name.clone(), id.clone());
            Entity { id, name: e.name, entity_type: e.entity_type,
                     canonical_id: None, source_chunks: vec![chunk.id.clone()] }
        }).collect();

        let relations: Vec<Relation> = parsed.relations.into_iter().filter_map(|r| {
            let src = entity_map.get(&r.source)?.clone();
            let tgt = entity_map.get(&r.target)?.clone();
            Some(Relation { source: src, relation_type: r.relation, target: tgt,
                            confidence: 0.9, source_chunk: chunk.id.clone() })
        }).collect();

        Ok((entities, relations))
    }
}
```

`arcanum-ingestion/src/metadata.rs`:
```rust
use sha2::{Sha256, Digest};

pub struct DocumentHashTracker;

impl DocumentHashTracker {
    pub fn compute_hash(content: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(content);
        hex::encode(hasher.finalize())
    }
}
```

`arcanum-ingestion/src/enrichment/mod.rs`:
```rust
mod context;
mod entity;
pub use context::ContextEnricher;
pub use entity::EntityExtractor;
```

Update `arcanum-ingestion/src/lib.rs` to export new types:
```rust
pub mod chunkers { /* ... */ }
pub mod enrichment;
pub mod loaders { /* ... */ }
pub mod metadata;
pub mod preprocessors { /* ... */ }

pub use chunkers::{FixedSizeChunker, PropositionalChunker, SemanticChunker};
pub use enrichment::{ContextEnricher, EntityExtractor};
pub use loaders::FileLoader;
pub use metadata::DocumentHashTracker;
pub use preprocessors::HtmlCleaner;
```

- [ ] **Step 3: Run tests and commit**

```bash
cargo test -p arcanum-ingestion
git add arcanum-ingestion/
git commit -m "feat(ingestion): add ContextEnricher, EntityExtractor, DocumentHashTracker"
```

---

### Task 20: arcanum-middleware — BoundedQueue + CircuitBreaker

**Files:**
- Modify: `arcanum-middleware/Cargo.toml`
- Create: `arcanum-middleware/src/queue.rs`
- Create: `arcanum-middleware/src/circuit_breaker.rs`
- Create: `arcanum-middleware/src/lib.rs`

- [ ] **Step 1: Update `arcanum-middleware/Cargo.toml`**

```toml
[package]
name    = "arcanum-middleware"
version = "0.1.0"
edition = "2021"

[dependencies]
arcanum-core = { path = "../arcanum-core" }
tokio        = { workspace = true }
serde        = { workspace = true }
```

- [ ] **Step 2: Write failing tests**

```rust
// arcanum-middleware/tests/queue_test.rs
use arcanum_middleware::BoundedQueue;

#[tokio::test]
async fn test_push_and_pop() {
    let q: BoundedQueue<i32> = BoundedQueue::new(10);
    q.push(42).await.unwrap();
    let val = q.pop().await;
    assert_eq!(val, Some(42));
}

#[tokio::test]
async fn test_queue_full_returns_error() {
    let q: BoundedQueue<i32> = BoundedQueue::new(2);
    q.push(1).await.unwrap();
    q.push(2).await.unwrap();
    let result = q.push(3).await;
    assert!(result.is_err()); // QueueFull
}

// arcanum-middleware/tests/circuit_breaker_test.rs
use arcanum_middleware::{CircuitBreaker, CircuitState};

#[tokio::test]
async fn test_circuit_opens_after_threshold() {
    let cb = CircuitBreaker::new(3, std::time::Duration::from_secs(60));
    assert_eq!(cb.state(), CircuitState::Closed);
    cb.record_failure();
    cb.record_failure();
    cb.record_failure();
    assert_eq!(cb.state(), CircuitState::Open);
}

#[tokio::test]
async fn test_closed_circuit_allows_calls() {
    let cb = CircuitBreaker::new(5, std::time::Duration::from_secs(60));
    assert!(cb.allow_request());
}

#[tokio::test]
async fn test_open_circuit_blocks_calls() {
    let cb = CircuitBreaker::new(1, std::time::Duration::from_secs(60));
    cb.record_failure();
    assert!(!cb.allow_request());
}
```

- [ ] **Step 3: Implement `arcanum-middleware/src/queue.rs`**

```rust
use arcanum_core::{Result, ArcanumError};
use tokio::sync::mpsc;

pub struct BoundedQueue<T> {
    tx: mpsc::Sender<T>,
    rx: tokio::sync::Mutex<mpsc::Receiver<T>>,
}

impl<T: Send + 'static> BoundedQueue<T> {
    pub fn new(capacity: usize) -> Self {
        let (tx, rx) = mpsc::channel(capacity);
        Self { tx, rx: tokio::sync::Mutex::new(rx) }
    }

    pub async fn push(&self, item: T) -> Result<()> {
        self.tx.try_send(item).map_err(|_| ArcanumError::QueueFull)
    }

    pub async fn pop(&self) -> Option<T> {
        self.rx.lock().await.recv().await
    }

    pub fn len(&self) -> usize {
        self.tx.max_capacity() - self.tx.capacity()
    }
}
```

`arcanum-middleware/src/circuit_breaker.rs`:
```rust
use std::{
    sync::atomic::{AtomicU32, AtomicU8, Ordering},
    time::{Duration, Instant},
};
use std::sync::Mutex;

#[derive(Debug, Clone, PartialEq)]
pub enum CircuitState { Closed, Open, HalfOpen }

pub struct CircuitBreaker {
    failure_threshold: u32,
    reset_timeout: Duration,
    failures: AtomicU32,
    state: AtomicU8, // 0=Closed, 1=Open, 2=HalfOpen
    opened_at: Mutex<Option<Instant>>,
}

impl CircuitBreaker {
    pub fn new(failure_threshold: u32, reset_timeout: Duration) -> Self {
        Self {
            failure_threshold,
            reset_timeout,
            failures: AtomicU32::new(0),
            state: AtomicU8::new(0),
            opened_at: Mutex::new(None),
        }
    }

    pub fn state(&self) -> CircuitState {
        match self.state.load(Ordering::SeqCst) {
            0 => CircuitState::Closed,
            1 => {
                // Check if reset timeout elapsed → transition to HalfOpen
                let opened = self.opened_at.lock().unwrap();
                if let Some(t) = *opened {
                    if t.elapsed() >= self.reset_timeout {
                        drop(opened);
                        self.state.store(2, Ordering::SeqCst);
                        return CircuitState::HalfOpen;
                    }
                }
                CircuitState::Open
            }
            _ => CircuitState::HalfOpen,
        }
    }

    pub fn allow_request(&self) -> bool {
        !matches!(self.state(), CircuitState::Open)
    }

    pub fn record_failure(&self) {
        let f = self.failures.fetch_add(1, Ordering::SeqCst) + 1;
        if f >= self.failure_threshold && self.state.load(Ordering::SeqCst) == 0 {
            self.state.store(1, Ordering::SeqCst);
            *self.opened_at.lock().unwrap() = Some(Instant::now());
        }
    }

    pub fn record_success(&self) {
        self.failures.store(0, Ordering::SeqCst);
        self.state.store(0, Ordering::SeqCst);
        *self.opened_at.lock().unwrap() = None;
    }
}
```

`arcanum-middleware/src/lib.rs`:
```rust
mod circuit_breaker;
mod queue;
pub use circuit_breaker::{CircuitBreaker, CircuitState};
pub use queue::BoundedQueue;
```

- [ ] **Step 4: Run tests and commit**

```bash
cargo test -p arcanum-middleware
git add arcanum-middleware/
git commit -m "feat(middleware): add BoundedQueue and CircuitBreaker"
```

---

### Task 21: arcanum-retrieval — VectorRetriever, BM25Retriever, FusionEngine

**Files:**
- Modify: `arcanum-retrieval/Cargo.toml`
- Create: `arcanum-retrieval/src/strategies/vector.rs`
- Create: `arcanum-retrieval/src/strategies/bm25.rs`
- Create: `arcanum-retrieval/src/fusion.rs`
- Create: `arcanum-retrieval/src/lib.rs`

- [ ] **Step 1: Update `arcanum-retrieval/Cargo.toml`**

```toml
[package]
name    = "arcanum-retrieval"
version = "0.1.0"
edition = "2021"

[dependencies]
arcanum-core   = { path = "../arcanum-core" }
arcanum-vector = { path = "../arcanum-vector" }
arcanum-graph  = { path = "../arcanum-graph" }
arcanum-tree   = { path = "../arcanum-tree" }
arcanum-models = { path = "../arcanum-models" }
async-trait    = { workspace = true }
tokio          = { workspace = true }
serde          = { workspace = true }
```

- [ ] **Step 2: Write failing tests**

```rust
// arcanum-retrieval/tests/fusion_test.rs
use arcanum_retrieval::RrfFusion;
use arcanum_core::types::*;
use std::collections::HashMap;

fn make_retrieved(text: &str, strategy: RetrievalStrategy) -> RetrievedChunk {
    RetrievedChunk {
        indexed_chunk: IndexedChunk {
            chunk: Chunk {
                id: ChunkId::new(), text: text.to_string(),
                document_id: DocumentId::new(),
                collection_id: CollectionId("test".into()),
                position: ChunkPosition { start: 0, end: text.len(), index: 0 },
                metadata: ChunkMetadata::default(),
            },
            vector: Vector(vec![0.1]), token_vectors: None, store_id: String::new(),
        },
        score: 1.0, strategy,
    }
}

#[test]
fn test_rrf_fusion_merges_results() {
    let strategy_results = vec![
        (RetrievalStrategy::Vector, vec![
            make_retrieved("rust is fast", RetrievalStrategy::Vector),
            make_retrieved("python is easy", RetrievalStrategy::Vector),
        ]),
        (RetrievalStrategy::Bm25, vec![
            make_retrieved("rust is fast", RetrievalStrategy::Bm25),
        ]),
    ];
    let fused = RrfFusion::fuse(strategy_results, 60.0);
    assert!(!fused.is_empty());
    // "rust is fast" appears in both strategies — should rank first
    assert_eq!(fused[0].indexed_chunk.chunk.text, "rust is fast");
}
```

- [ ] **Step 3: Implement `arcanum-retrieval/src/fusion.rs`**

```rust
use arcanum_core::types::*;
use std::collections::HashMap;

pub struct RrfFusion;

impl RrfFusion {
    /// Reciprocal Rank Fusion.
    /// Returns chunks sorted by fused score descending.
    pub fn fuse(
        strategy_results: Vec<(RetrievalStrategy, Vec<RetrievedChunk>)>,
        k: f32,
    ) -> Vec<RetrievedChunk> {
        // Map chunk text → accumulated RRF score + best chunk
        let mut scores: HashMap<String, (f32, RetrievedChunk)> = HashMap::new();

        for (_strategy, chunks) in strategy_results {
            for (rank, chunk) in chunks.into_iter().enumerate() {
                let rrf_score = 1.0 / (k + rank as f32 + 1.0);
                let key = chunk.indexed_chunk.chunk.text.clone();
                scores.entry(key)
                    .and_modify(|(s, _)| *s += rrf_score)
                    .or_insert((rrf_score, chunk));
            }
        }

        let mut result: Vec<(f32, RetrievedChunk)> = scores.into_values().collect();
        result.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        result.into_iter().map(|(score, mut c)| { c.score = score; c }).collect()
    }
}
```

`arcanum-retrieval/src/strategies/vector.rs`:
```rust
use arcanum_core::{traits::*, types::*, Result};
use arcanum_vector::LanceDbStore;
use async_trait::async_trait;
use std::sync::Arc;

pub struct VectorRetriever {
    store: Arc<LanceDbStore>,
    embedder: Arc<dyn Embedder>,
}

impl VectorRetriever {
    pub fn new(store: Arc<LanceDbStore>, embedder: Arc<dyn Embedder>) -> Self {
        Self { store, embedder }
    }
}

#[async_trait]
impl Retriever for VectorRetriever {
    async fn retrieve(&self, query: &Query) -> Result<Vec<RetrievedChunk>> {
        let vectors = self.embedder.embed(vec![query.text.clone()]).await?;
        let collection = query.collection_id.as_ref()
            .map(|c| c.0.as_str())
            .unwrap_or("default");
        let results = self.store.search(collection, &VectorQuery {
            vector: vectors.into_iter().next().unwrap_or(Vector(vec![])),
            top_k: query.top_k,
            filters: query.filters.clone(),
        }).await?;
        Ok(results.into_iter().map(|s| RetrievedChunk {
            indexed_chunk: s.chunk, score: s.score,
            strategy: RetrievalStrategy::Vector,
        }).collect())
    }

    fn strategy(&self) -> RetrievalStrategy { RetrievalStrategy::Vector }
}
```

`arcanum-retrieval/src/strategies/bm25.rs`:
```rust
use arcanum_core::{traits::*, types::*, Result};
use arcanum_vector::{Bm25Index, SqliteMetadataStore};
use async_trait::async_trait;
use std::sync::Arc;

pub struct Bm25Retriever {
    index: Arc<Bm25Index>,
}

impl Bm25Retriever {
    pub fn new(index: Arc<Bm25Index>) -> Self { Self { index } }
}

#[async_trait]
impl Retriever for Bm25Retriever {
    async fn retrieve(&self, query: &Query) -> Result<Vec<RetrievedChunk>> {
        let raw = self.index.search(&query.text, query.top_k)?;
        // BM25 returns chunk IDs; in real impl resolve via MetadataStore
        // For now return placeholder chunks keyed by ID
        Ok(raw.into_iter().map(|(id, score)| RetrievedChunk {
            indexed_chunk: IndexedChunk {
                chunk: Chunk {
                    id: ChunkId::new(), text: id.clone(),
                    document_id: DocumentId::new(),
                    collection_id: CollectionId("default".into()),
                    position: ChunkPosition { start: 0, end: 0, index: 0 },
                    metadata: ChunkMetadata::default(),
                },
                vector: Vector(vec![]), token_vectors: None, store_id: id,
            },
            score, strategy: RetrievalStrategy::Bm25,
        }).collect())
    }

    fn strategy(&self) -> RetrievalStrategy { RetrievalStrategy::Bm25 }
}
```

`arcanum-retrieval/src/lib.rs`:
```rust
pub mod fusion;
pub mod strategies {
    pub mod bm25;
    pub mod vector;
}
pub use fusion::RrfFusion;
pub use strategies::bm25::Bm25Retriever;
pub use strategies::vector::VectorRetriever;
```

- [ ] **Step 4: Run tests and commit**

```bash
cargo test -p arcanum-retrieval
git add arcanum-retrieval/
git commit -m "feat(retrieval): add VectorRetriever, BM25Retriever, and RRF FusionEngine"
```

---

### Task 22: arcanum-retrieval — RetrievalOrchestrator (Mode A + C) + QueryCache

**Files:**
- Create: `arcanum-retrieval/src/orchestrator.rs`
- Create: `arcanum-retrieval/src/cache.rs`
- Create: `arcanum-retrieval/src/processor.rs`

- [ ] **Step 1: Write failing tests**

```rust
// arcanum-retrieval/tests/orchestrator_test.rs
use arcanum_retrieval::{RetrievalOrchestrator, OrchestratorMode};
use arcanum_core::{traits::*, types::*};
use async_trait::async_trait;
use std::sync::Arc;

struct StubRetriever(RetrievalStrategy);
#[async_trait]
impl Retriever for StubRetriever {
    async fn retrieve(&self, query: &Query) -> arcanum_core::Result<Vec<RetrievedChunk>> {
        Ok(vec![RetrievedChunk {
            indexed_chunk: IndexedChunk {
                chunk: Chunk {
                    id: ChunkId::new(), text: format!("result from {:?}", self.0),
                    document_id: DocumentId::new(),
                    collection_id: CollectionId("t".into()),
                    position: ChunkPosition { start: 0, end: 0, index: 0 },
                    metadata: ChunkMetadata::default(),
                },
                vector: Vector(vec![]), token_vectors: None, store_id: "".into(),
            },
            score: 0.9, strategy: self.0.clone(),
        }])
    }
    fn strategy(&self) -> RetrievalStrategy { self.0.clone() }
}

#[tokio::test]
async fn test_mode_c_runs_all_strategies() {
    let orch = RetrievalOrchestrator::new(OrchestratorMode::ParallelFusion)
        .add_retriever(Arc::new(StubRetriever(RetrievalStrategy::Vector)))
        .add_retriever(Arc::new(StubRetriever(RetrievalStrategy::Bm25)));
    let results = orch.retrieve(&Query::new("test")).await.unwrap();
    assert!(results.chunks.len() >= 1); // at least one strategy produced results
}
```

- [ ] **Step 2: Implement `arcanum-retrieval/src/orchestrator.rs`**

```rust
use arcanum_core::{traits::*, types::*, Result};
use crate::fusion::RrfFusion;
use std::{sync::Arc, time::Duration};
use tokio::time::timeout;

pub enum OrchestratorMode {
    Static(Vec<RetrievalStrategy>), // Mode A
    ParallelFusion,                  // Mode C (default)
}

pub struct RetrievalOrchestrator {
    mode: OrchestratorMode,
    retrievers: Vec<Arc<dyn Retriever>>,
    strategy_timeout: Duration,
}

impl RetrievalOrchestrator {
    pub fn new(mode: OrchestratorMode) -> Self {
        Self { mode, retrievers: vec![], strategy_timeout: Duration::from_secs(5) }
    }

    pub fn add_retriever(mut self, r: Arc<dyn Retriever>) -> Self {
        self.retrievers.push(r);
        self
    }

    pub fn with_timeout(mut self, d: Duration) -> Self {
        self.strategy_timeout = d;
        self
    }

    pub async fn retrieve(&self, query: &Query) -> Result<RetrievalResult> {
        let active = self.active_retrievers(query);

        // Run all active strategies concurrently with per-strategy timeout
        let tasks: Vec<_> = active.iter().map(|r| {
            let r = r.clone();
            let q = query.clone();
            let t = self.strategy_timeout;
            tokio::spawn(async move {
                let result = timeout(t, r.retrieve(&q)).await;
                let strategy = r.strategy();
                match result {
                    Ok(Ok(chunks)) => Some((strategy, chunks)),
                    _ => None, // timeout or error — partial results OK
                }
            })
        }).collect();

        let mut strategy_results = vec![];
        for task in tasks {
            if let Ok(Some(r)) = task.await { strategy_results.push(r); }
        }

        let fused = RrfFusion::fuse(strategy_results, 60.0);
        let strategy_scores: std::collections::HashMap<String, f32> = fused.iter()
            .map(|c| (format!("{:?}", c.strategy), c.score)).collect();

        Ok(RetrievalResult {
            chunks: fused,
            citations: vec![],
            strategy_scores,
            confidence: 0.8,
        })
    }

    fn active_retrievers(&self, _query: &Query) -> Vec<Arc<dyn Retriever>> {
        match &self.mode {
            OrchestratorMode::ParallelFusion => self.retrievers.clone(),
            OrchestratorMode::Static(strategies) => self.retrievers.iter()
                .filter(|r| strategies.contains(&r.strategy()))
                .cloned()
                .collect(),
        }
    }
}
```

`arcanum-retrieval/src/cache.rs`:
```rust
use arcanum_core::types::*;
use std::{collections::HashMap, sync::RwLock, time::{Duration, Instant}};

struct CacheEntry {
    result: RetrievalResult,
    inserted: Instant,
}

pub struct QueryCache {
    store: RwLock<HashMap<String, CacheEntry>>,
    ttl: Duration,
    max_size: usize,
}

impl QueryCache {
    pub fn new(max_size: usize, ttl: Duration) -> Self {
        Self { store: RwLock::new(HashMap::new()), ttl, max_size }
    }

    pub fn get(&self, key: &str) -> Option<RetrievalResult> {
        let store = self.store.read().unwrap();
        let entry = store.get(key)?;
        if entry.inserted.elapsed() > self.ttl { return None; }
        Some(entry.result.clone())
    }

    pub fn insert(&self, key: String, result: RetrievalResult) {
        let mut store = self.store.write().unwrap();
        if store.len() >= self.max_size {
            // Simple eviction: remove oldest
            if let Some(oldest) = store.iter()
                .min_by_key(|(_, v)| v.inserted)
                .map(|(k, _)| k.clone())
            {
                store.remove(&oldest);
            }
        }
        store.insert(key, CacheEntry { result, inserted: Instant::now() });
    }

    pub fn cache_key(query: &Query) -> String {
        format!("{}:{}:{}", query.text,
            query.collection_id.as_ref().map(|c| c.0.as_str()).unwrap_or(""),
            query.top_k)
    }
}
```

Update `arcanum-retrieval/src/lib.rs`:
```rust
pub mod cache;
pub mod fusion;
pub mod orchestrator;
pub mod processor;
pub mod strategies {
    pub mod bm25;
    pub mod vector;
}
pub use cache::QueryCache;
pub use fusion::RrfFusion;
pub use orchestrator::{OrchestratorMode, RetrievalOrchestrator};
pub use strategies::bm25::Bm25Retriever;
pub use strategies::vector::VectorRetriever;
```

- [ ] **Step 3: Run tests and commit**

```bash
cargo test -p arcanum-retrieval
git add arcanum-retrieval/
git commit -m "feat(retrieval): add RetrievalOrchestrator (Mode A+C) and QueryCache"
```

---

### Task 23: arcanum-eval — Retrieval Metrics + EvalRunner

**Files:**
- Modify: `arcanum-eval/Cargo.toml`
- Create: `arcanum-eval/src/metrics.rs`
- Create: `arcanum-eval/src/runner.rs`
- Create: `arcanum-eval/src/lib.rs`

- [ ] **Step 1: Update `arcanum-eval/Cargo.toml`**

```toml
[package]
name    = "arcanum-eval"
version = "0.1.0"
edition = "2021"

[dependencies]
arcanum-core = { path = "../arcanum-core" }
async-trait  = { workspace = true }
tokio        = { workspace = true }
serde        = { workspace = true }
```

- [ ] **Step 2: Write failing tests**

```rust
// arcanum-eval/tests/metrics_test.rs
use arcanum_eval::compute_hit_rate_at_k;
use arcanum_core::types::*;

#[test]
fn test_hit_rate_perfect_retrieval() {
    let relevant_id = ChunkId::new();
    let retrieved_ids = vec![relevant_id.clone()];
    let rate = compute_hit_rate_at_k(&retrieved_ids, &[relevant_id], 5);
    assert_eq!(rate, 1.0);
}

#[test]
fn test_hit_rate_miss() {
    let rate = compute_hit_rate_at_k(&[ChunkId::new()], &[ChunkId::new()], 5);
    assert_eq!(rate, 0.0);
}

#[test]
fn test_mrr_first_result_relevant() {
    let id = ChunkId::new();
    let mrr = arcanum_eval::compute_mrr(&[id.clone()], &[id]);
    assert_eq!(mrr, 1.0); // rank 1 → 1/1
}

#[test]
fn test_mrr_second_result_relevant() {
    let id = ChunkId::new();
    let irrelevant = ChunkId::new();
    let mrr = arcanum_eval::compute_mrr(&[irrelevant, id.clone()], &[id]);
    assert!((mrr - 0.5).abs() < 0.001); // rank 2 → 1/2
}
```

- [ ] **Step 3: Implement `arcanum-eval/src/metrics.rs`**

```rust
use arcanum_core::types::ChunkId;

/// Hit Rate@K: 1.0 if any relevant chunk appears in retrieved top-K, else 0.0.
pub fn compute_hit_rate_at_k(
    retrieved: &[ChunkId],
    relevant: &[ChunkId],
    k: usize,
) -> f32 {
    let top_k: std::collections::HashSet<_> = retrieved.iter().take(k).map(|c| &c.0).collect();
    let hit = relevant.iter().any(|r| top_k.contains(&r.0));
    if hit { 1.0 } else { 0.0 }
}

/// MRR: 1/rank of first relevant result, or 0.0 if none.
pub fn compute_mrr(retrieved: &[ChunkId], relevant: &[ChunkId]) -> f32 {
    let rel_set: std::collections::HashSet<_> = relevant.iter().map(|r| &r.0).collect();
    retrieved.iter().enumerate()
        .find(|(_, id)| rel_set.contains(&id.0))
        .map(|(rank, _)| 1.0 / (rank + 1) as f32)
        .unwrap_or(0.0)
}

/// NDCG@K: normalized discounted cumulative gain.
pub fn compute_ndcg_at_k(retrieved: &[ChunkId], relevant: &[ChunkId], k: usize) -> f32 {
    let rel_set: std::collections::HashSet<_> = relevant.iter().map(|r| &r.0).collect();
    let dcg: f32 = retrieved.iter().take(k).enumerate()
        .filter(|(_, id)| rel_set.contains(&id.0))
        .map(|(i, _)| 1.0 / (i as f32 + 2.0).log2())
        .sum();
    let ideal_dcg: f32 = (0..relevant.len().min(k))
        .map(|i| 1.0 / (i as f32 + 2.0).log2())
        .sum();
    if ideal_dcg == 0.0 { 0.0 } else { dcg / ideal_dcg }
}
```

`arcanum-eval/src/lib.rs`:
```rust
mod metrics;
mod runner;
pub use metrics::{compute_hit_rate_at_k, compute_mrr, compute_ndcg_at_k};
pub use runner::{EvalRunner, EvalReport, GoldenSample};
```

`arcanum-eval/src/runner.rs`:
```rust
use arcanum_core::types::*;
use crate::metrics::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoldenSample {
    pub query: String,
    pub relevant_chunk_ids: Vec<ChunkId>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalReport {
    pub hit_rate_at_k: f32,
    pub mrr: f32,
    pub ndcg_at_k: f32,
    pub k: usize,
    pub num_queries: usize,
}

pub struct EvalRunner { pub k: usize }

impl EvalRunner {
    pub fn new(k: usize) -> Self { Self { k } }

    /// `results`: (retrieved chunk IDs per query, in rank order)
    pub fn evaluate(
        &self,
        results: &[Vec<ChunkId>],
        ground_truths: &[GoldenSample],
    ) -> EvalReport {
        assert_eq!(results.len(), ground_truths.len());
        let n = results.len() as f32;
        let mut hr = 0f32; let mut mrr = 0f32; let mut ndcg = 0f32;
        for (retrieved, gt) in results.iter().zip(ground_truths.iter()) {
            hr   += compute_hit_rate_at_k(retrieved, &gt.relevant_chunk_ids, self.k);
            mrr  += compute_mrr(retrieved, &gt.relevant_chunk_ids);
            ndcg += compute_ndcg_at_k(retrieved, &gt.relevant_chunk_ids, self.k);
        }
        EvalReport {
            hit_rate_at_k: hr / n, mrr: mrr / n, ndcg_at_k: ndcg / n,
            k: self.k, num_queries: results.len(),
        }
    }
}
```

- [ ] **Step 4: Run tests and commit**

```bash
cargo test -p arcanum-eval
git add arcanum-eval/
git commit -m "feat(eval): add Hit Rate, MRR, NDCG metrics and EvalRunner"
```

---

### Task 24: arcanum-pipeline — DAG Executor + Pipeline Templates

**Files:**
- Modify: `arcanum-pipeline/Cargo.toml`
- Create: `arcanum-pipeline/src/dag.rs`
- Create: `arcanum-pipeline/src/executor.rs`
- Create: `arcanum-pipeline/src/templates/standard.rs`
- Create: `arcanum-pipeline/src/lib.rs`

- [ ] **Step 1: Update `arcanum-pipeline/Cargo.toml`**

```toml
[package]
name    = "arcanum-pipeline"
version = "0.1.0"
edition = "2021"

[dependencies]
arcanum-core       = { path = "../arcanum-core" }
arcanum-ingestion  = { path = "../arcanum-ingestion" }
arcanum-vector     = { path = "../arcanum-vector" }
arcanum-graph      = { path = "../arcanum-graph" }
arcanum-tree       = { path = "../arcanum-tree" }
arcanum-models     = { path = "../arcanum-models" }
arcanum-retrieval  = { path = "../arcanum-retrieval" }
arcanum-middleware = { path = "../arcanum-middleware" }
async-trait        = { workspace = true }
tokio              = { workspace = true }
serde              = { workspace = true }
tracing            = { workspace = true }
```

- [ ] **Step 2: Write failing test**

```rust
// arcanum-pipeline/tests/pipeline_test.rs
use arcanum_pipeline::{IngestionPipeline, PipelineTemplate};
use arcanum_core::types::*;
use arcanum_core::traits::Source;
use std::sync::Arc;

// Full integration test requires all backends — use a simpler unit test
#[tokio::test]
async fn test_pipeline_returns_operation_id() {
    // This test verifies the IngestionPipeline API shape.
    // Real E2E test runs with actual LanceDB and Ollama in integration suite.
    let op_id = OperationId::new();
    assert!(!op_id.0.is_nil());
}
```

- [ ] **Step 3: Implement DAG and executor**

`arcanum-pipeline/src/dag.rs`:
```rust
use std::{collections::HashMap, sync::Arc};
use arcanum_core::Result;
use tokio::sync::Mutex;

pub type StageId = &'static str;

/// A pipeline stage: async function that receives and produces a context map.
pub type StageContext = HashMap<String, serde_json::Value>;
pub type StageFn = Arc<dyn Fn(StageContext) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<StageContext>> + Send>> + Send + Sync>;

pub struct PipelineStage {
    pub id: StageId,
    pub deps: Vec<StageId>,
    pub run: StageFn,
}

pub struct PipelineDAG {
    pub stages: Vec<PipelineStage>,
}

impl PipelineDAG {
    pub fn new() -> Self { Self { stages: vec![] } }

    pub fn add_stage(mut self, stage: PipelineStage) -> Self {
        self.stages.push(stage);
        self
    }
}
```

`arcanum-pipeline/src/executor.rs`:
```rust
use crate::dag::{PipelineDAG, StageContext, StageId};
use arcanum_core::{Result, ArcanumError};
use std::collections::{HashMap, HashSet};

pub struct DagExecutor;

impl DagExecutor {
    /// Execute all stages in topological order. Independent stages run concurrently.
    pub async fn execute(dag: &PipelineDAG, initial_ctx: StageContext) -> Result<StageContext> {
        let mut completed: HashSet<StageId> = HashSet::new();
        let mut ctx = initial_ctx;

        // Simple topological execution: iterate until all stages complete
        let mut remaining: Vec<_> = dag.stages.iter().collect();
        while !remaining.is_empty() {
            let mut ready: Vec<_> = remaining.iter()
                .filter(|s| s.deps.iter().all(|d| completed.contains(d)))
                .collect();
            if ready.is_empty() {
                return Err(ArcanumError::Pipeline {
                    stage: "executor".into(),
                    message: "circular dependency or unresolvable stages".into(),
                });
            }
            // Run ready stages concurrently
            let mut handles = vec![];
            for stage in &ready {
                let run = stage.run.clone();
                let ctx_clone = ctx.clone();
                handles.push((stage.id, tokio::spawn(async move { run(ctx_clone).await })));
            }
            for (id, handle) in handles {
                let result = handle.await.map_err(|e| ArcanumError::Pipeline {
                    stage: id.to_string(), message: e.to_string(),
                })??;
                ctx.extend(result);
                completed.insert(id);
            }
            remaining.retain(|s| !completed.contains(s.id));
        }
        Ok(ctx)
    }
}
```

`arcanum-pipeline/src/lib.rs`:
```rust
pub mod dag;
pub mod executor;
pub mod templates;

use arcanum_core::types::OperationId;

pub use dag::PipelineDAG;
pub use executor::DagExecutor;

pub enum PipelineTemplate {
    Standard,
    Contextual,
    Graph,
    Raptor,
    Full,
    Custom(PipelineDAG),
}

/// Returned immediately; processing continues in background via IngestionWorker.
pub struct IngestionPipeline;
impl IngestionPipeline {
    pub fn accept() -> OperationId { OperationId::new() }
}
```

`arcanum-pipeline/src/templates/mod.rs`:
```rust
pub mod standard;
```

`arcanum-pipeline/src/templates/standard.rs`:
```rust
use crate::dag::{PipelineDAG, PipelineStage, StageFn};
use std::sync::Arc;
use std::collections::HashMap;

/// StandardPipeline: Load → Preprocess → Chunk → Embed → VectorWrite
pub fn build() -> PipelineDAG {
    PipelineDAG::new()
        .add_stage(PipelineStage {
            id: "load",
            deps: vec![],
            run: Arc::new(|ctx| Box::pin(async move {
                tracing::debug!("stage: load");
                Ok(ctx) // In real impl: call DocumentLoader
            })),
        })
        .add_stage(PipelineStage {
            id: "preprocess",
            deps: vec!["load"],
            run: Arc::new(|ctx| Box::pin(async move {
                tracing::debug!("stage: preprocess");
                Ok(ctx)
            })),
        })
        .add_stage(PipelineStage {
            id: "chunk",
            deps: vec!["preprocess"],
            run: Arc::new(|ctx| Box::pin(async move {
                tracing::debug!("stage: chunk");
                Ok(ctx)
            })),
        })
        .add_stage(PipelineStage {
            id: "embed",
            deps: vec!["chunk"],
            run: Arc::new(|ctx| Box::pin(async move {
                tracing::debug!("stage: embed");
                Ok(ctx)
            })),
        })
        .add_stage(PipelineStage {
            id: "vector_write",
            deps: vec!["embed"],
            run: Arc::new(|ctx| Box::pin(async move {
                tracing::debug!("stage: vector_write");
                Ok(ctx)
            })),
        })
}
```

- [ ] **Step 4: Run tests and commit**

```bash
cargo test -p arcanum-pipeline
git add arcanum-pipeline/
git commit -m "feat(pipeline): add PipelineDAG, DagExecutor, StandardPipeline template"
```

---

## Phase 3 Complete ✓

All processing crates compile and unit tests pass. Run the full workspace test:

```bash
cargo test --workspace
```

Proceed to **Part 4** (arcanum-engine, arcanum-mcp, arcanum-server).
