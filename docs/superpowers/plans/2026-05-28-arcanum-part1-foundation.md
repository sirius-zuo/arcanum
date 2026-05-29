# Arcanum Part 1 — Foundation (Workspace + arcanum-core)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Stand up the Cargo workspace and implement `arcanum-core` — all traits, domain types, config, and error types that every other crate depends on.

**Architecture:** Single `arcanum-core` crate with no external network dependencies. All traits use `async-trait`. Domain types are plain Rust structs/enums deriving `serde`. Config uses a layered system (defaults → file → env → runtime).

**Tech Stack:** Rust 1.78+, `async-trait 0.1`, `serde 1`, `thiserror 2`, `uuid 1`, `chrono 0.4`, `tokio 1` (dev only)

**Sequence:** Part 1 → Part 2 (stores & models) → Part 3 (processing) → Part 4 (service)

---

### Task 1: Cargo Workspace Setup

**Files:**
- Create: `Cargo.toml` (workspace root)
- Create: `arcanum-core/Cargo.toml`
- Create: `arcanum-core/src/lib.rs`
- Create stubs: `arcanum-{vector,graph,tree,models,ingestion,middleware,retrieval,eval,pipeline,engine,mcp,server}/Cargo.toml` + `src/lib.rs` for each

- [ ] **Step 1: Create workspace root `Cargo.toml`**

```toml
[workspace]
members = [
    "arcanum-core",
    "arcanum-vector",
    "arcanum-graph",
    "arcanum-tree",
    "arcanum-models",
    "arcanum-ingestion",
    "arcanum-middleware",
    "arcanum-retrieval",
    "arcanum-eval",
    "arcanum-pipeline",
    "arcanum-engine",
    "arcanum-mcp",
    "arcanum-server",
]
resolver = "2"

[workspace.dependencies]
async-trait  = "0.1"
tokio        = { version = "1", features = ["full"] }
serde        = { version = "1", features = ["derive"] }
serde_json   = "1"
thiserror    = "2"
anyhow       = "1"
uuid         = { version = "1", features = ["v4", "serde"] }
chrono       = { version = "0.4", features = ["serde"] }
tracing      = "0.1"
```

- [ ] **Step 2: Create `arcanum-core/Cargo.toml`**

```toml
[package]
name    = "arcanum-core"
version = "0.1.0"
edition = "2021"

[dependencies]
async-trait = { workspace = true }
serde       = { workspace = true }
serde_json  = { workspace = true }
thiserror   = { workspace = true }
anyhow      = { workspace = true }
uuid        = { workspace = true }
chrono      = { workspace = true }

[dev-dependencies]
tokio = { workspace = true }
```

- [ ] **Step 3: Create stub crate manifests**

For each of the 12 remaining crates, create `<crate>/Cargo.toml`:

```toml
# Example: arcanum-vector/Cargo.toml
[package]
name    = "arcanum-vector"
version = "0.1.0"
edition = "2021"

[dependencies]
arcanum-core = { path = "../arcanum-core" }
```

And `<crate>/src/lib.rs`:
```rust
// placeholder
```

Repeat for: `arcanum-graph`, `arcanum-tree`, `arcanum-models`, `arcanum-ingestion`, `arcanum-middleware`, `arcanum-retrieval`, `arcanum-eval`, `arcanum-pipeline`, `arcanum-engine`, `arcanum-mcp`, `arcanum-server`.

- [ ] **Step 4: Verify workspace builds**

```bash
cargo build --workspace
```
Expected: all 13 crates compile (empty libs are fine).

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml arcanum-*/Cargo.toml arcanum-*/src/lib.rs
git commit -m "chore: initialize arcanum cargo workspace with 13 crate stubs"
```

---

### Task 2: Error Types

**Files:**
- Create: `arcanum-core/src/error.rs`
- Modify: `arcanum-core/src/lib.rs`

- [ ] **Step 1: Write the failing test**

In `arcanum-core/src/error.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let e = ArcanumError::Storage("connection refused".into());
        assert_eq!(e.to_string(), "storage error: connection refused");
    }

    #[test]
    fn test_queue_full_display() {
        assert_eq!(ArcanumError::QueueFull.to_string(), "queue full");
    }

    #[test]
    fn test_result_type() {
        let ok: Result<i32> = Ok(42);
        assert!(ok.is_ok());
        let err: Result<i32> = Err(ArcanumError::NotFound("chunk".into()));
        assert!(err.is_err());
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cargo test -p arcanum-core
```
Expected: compile error — `ArcanumError` not defined.

- [ ] **Step 3: Implement `arcanum-core/src/error.rs`**

```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ArcanumError {
    #[error("storage error: {0}")]
    Storage(String),
    #[error("embedding error: {0}")]
    Embedding(String),
    #[error("enrichment error: {0}")]
    Enrichment(String),
    #[error("ingestion error: {0}")]
    Ingestion(String),
    #[error("retrieval error: {0}")]
    Retrieval(String),
    #[error("configuration error: {0}")]
    Config(String),
    #[error("authentication error: {0}")]
    Auth(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("queue full")]
    QueueFull,
    #[error("pipeline error in stage '{stage}': {message}")]
    Pipeline { stage: String, message: String },
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

pub type Result<T> = std::result::Result<T, ArcanumError>;
```

Update `arcanum-core/src/lib.rs`:
```rust
pub mod error;
pub use error::{ArcanumError, Result};
```

- [ ] **Step 4: Run tests and verify pass**

```bash
cargo test -p arcanum-core
```
Expected: 3 tests pass.

- [ ] **Step 5: Commit**

```bash
git add arcanum-core/src/
git commit -m "feat(core): add ArcanumError and Result type"
```

---

### Task 3: Core Domain Types — Documents and Chunks

**Files:**
- Create: `arcanum-core/src/types/mod.rs`
- Create: `arcanum-core/src/types/document.rs`

- [ ] **Step 1: Write the failing test**

```rust
// arcanum-core/src/types/document.rs (test section)
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chunk_id_is_unique() {
        let a = ChunkId::new();
        let b = ChunkId::new();
        assert_ne!(a, b);
    }

    #[test]
    fn test_chunk_construction() {
        let chunk = Chunk {
            id: ChunkId::new(),
            text: "Hello world".to_string(),
            document_id: DocumentId::new(),
            collection_id: CollectionId("default".to_string()),
            position: ChunkPosition { start: 0, end: 11, index: 0 },
            metadata: ChunkMetadata::default(),
        };
        assert_eq!(chunk.text, "Hello world");
        assert_eq!(chunk.position.index, 0);
    }

    #[test]
    fn test_raw_document_hash() {
        let doc = RawDocument {
            id: DocumentId::new(),
            content: b"test content".to_vec(),
            mime_type: "text/plain".to_string(),
            source_uri: "file://test.txt".to_string(),
            metadata: Default::default(),
        };
        assert_eq!(doc.content_hash(), doc.content_hash()); // deterministic
    }
}
```

- [ ] **Step 2: Run to verify fail**

```bash
cargo test -p arcanum-core 2>&1 | head -20
```
Expected: compile error — types not defined.

- [ ] **Step 3: Implement `arcanum-core/src/types/document.rs`**

```rust
use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ChunkId(pub Uuid);
impl ChunkId {
    pub fn new() -> Self { Self(Uuid::new_v4()) }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DocumentId(pub Uuid);
impl DocumentId {
    pub fn new() -> Self { Self(Uuid::new_v4()) }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CollectionId(pub String);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawDocument {
    pub id: DocumentId,
    pub content: Vec<u8>,
    pub mime_type: String,
    pub source_uri: String,
    pub metadata: HashMap<String, String>,
}

impl RawDocument {
    pub fn content_hash(&self) -> String {
        use std::hash::{Hash, Hasher};
        use std::collections::hash_map::DefaultHasher;
        let mut h = DefaultHasher::new();
        self.content.hash(&mut h);
        format!("{:x}", h.finish())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkPosition {
    pub start: usize,
    pub end: usize,
    pub index: usize,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ChunkMetadata(pub HashMap<String, serde_json::Value>);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chunk {
    pub id: ChunkId,
    pub text: String,
    pub document_id: DocumentId,
    pub collection_id: CollectionId,
    pub position: ChunkPosition,
    pub metadata: ChunkMetadata,
}

/// A chunk that has been embedded — has one or more vectors.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexedChunk {
    pub chunk: Chunk,
    /// Primary dense vector (always present).
    pub vector: Vector,
    /// Token-level vectors for ColBERT (optional).
    pub token_vectors: Option<Vec<Vector>>,
    /// Backend-assigned store ID.
    pub store_id: String,
}

/// A chunk returned by a retrieval strategy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetrievedChunk {
    pub indexed_chunk: IndexedChunk,
    pub score: f32,
    pub strategy: RetrievalStrategy,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum RetrievalStrategy {
    Vector,
    Bm25,
    ColBert,
    Raptor,
    Graph,
}

/// A dense embedding vector.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Vector(pub Vec<f32>);
```

Update `arcanum-core/src/types/mod.rs`:
```rust
pub mod document;
pub use document::*;
```

Update `arcanum-core/src/lib.rs`:
```rust
pub mod error;
pub mod types;
pub use error::{ArcanumError, Result};
```

- [ ] **Step 4: Run tests**

```bash
cargo test -p arcanum-core
```
Expected: all tests pass.

- [ ] **Step 5: Commit**

```bash
git add arcanum-core/src/
git commit -m "feat(core): add document, chunk, and vector domain types"
```

---

### Task 4: Core Domain Types — Query, Graph, Tree, Operations

**Files:**
- Create: `arcanum-core/src/types/query.rs`
- Create: `arcanum-core/src/types/graph.rs`
- Create: `arcanum-core/src/types/tree.rs`
- Create: `arcanum-core/src/types/operation.rs`
- Create: `arcanum-core/src/types/enrichment.rs`

- [ ] **Step 1: Write the failing test**

```rust
// arcanum-core/src/types/query.rs
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_query_builder() {
        let q = Query::new("what is RAG?")
            .with_collection(CollectionId("docs".to_string()))
            .with_top_k(10);
        assert_eq!(q.text, "what is RAG?");
        assert_eq!(q.top_k, 10);
    }

    #[test]
    fn test_metadata_filter() {
        let f = MetadataFilter {
            field: "lang".to_string(),
            op: FilterOp::Eq,
            value: serde_json::json!("en"),
        };
        assert_eq!(f.field, "lang");
    }
}
```

- [ ] **Step 2: Implement query, graph, tree, operation, and enrichment types**

`arcanum-core/src/types/query.rs`:
```rust
use serde::{Deserialize, Serialize};
use super::document::CollectionId;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Query {
    pub text: String,
    pub collection_id: Option<CollectionId>,
    pub top_k: usize,
    pub filters: Vec<MetadataFilter>,
}

impl Query {
    pub fn new(text: impl Into<String>) -> Self {
        Self { text: text.into(), collection_id: None, top_k: 5, filters: vec![] }
    }
    pub fn with_collection(mut self, c: CollectionId) -> Self { self.collection_id = Some(c); self }
    pub fn with_top_k(mut self, k: usize) -> Self { self.top_k = k; self }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetadataFilter {
    pub field: String,
    pub op: FilterOp,
    pub value: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FilterOp { Eq, Ne, Gt, Lt, In }
```

`arcanum-core/src/types/graph.rs`:
```rust
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use super::document::ChunkId;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityId(pub Uuid);
impl EntityId { pub fn new() -> Self { Self(Uuid::new_v4()) } }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entity {
    pub id: EntityId,
    pub name: String,
    pub entity_type: String,
    pub canonical_id: Option<String>,
    pub source_chunks: Vec<ChunkId>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Relation {
    pub source: EntityId,
    pub relation_type: String,
    pub target: EntityId,
    pub confidence: f32,
    pub source_chunk: ChunkId,
}
```

`arcanum-core/src/types/tree.rs`:
```rust
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use super::document::Vector;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TreeNodeId(pub Uuid);
impl TreeNodeId { pub fn new() -> Self { Self(Uuid::new_v4()) } }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TreeNode {
    pub id: TreeNodeId,
    pub level: u32,
    pub text: String,
    pub vector: Vector,
    pub parent: Option<TreeNodeId>,
    pub children: Vec<TreeNodeId>,
    pub cluster_centroid: Option<Vector>,
}
```

`arcanum-core/src/types/operation.rs`:
```rust
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationId(pub Uuid);
impl OperationId { pub fn new() -> Self { Self(Uuid::new_v4()) } }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OperationStatus { Pending, Processing, PartialSuccess, Completed, Failed }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngestionReport {
    pub operation_id: OperationId,
    pub status: OperationStatus,
    pub stages_completed: Vec<String>,
    pub stages_failed: Vec<(String, String)>, // (stage, error)
    pub chunks_indexed: usize,
    pub duration_ms: u64,
    pub completed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Citation {
    pub document_uri: String,
    pub document_title: Option<String>,
    pub section: Option<String>,
    pub chunk_index: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetrievalResult {
    pub chunks: Vec<super::document::RetrievedChunk>,
    pub citations: Vec<Citation>,
    pub strategy_scores: std::collections::HashMap<String, f32>,
    pub confidence: f32,
}
```

`arcanum-core/src/types/enrichment.rs`:
```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnrichRequest {
    pub text: String,
    pub intent: EnrichIntent,
    pub context: Option<EnrichContext>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EnrichIntent {
    ContextPrefix,
    Summarize,
    ExtractEntities,
    Caption,
    Rerank,
    Custom(String),
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EnrichContext {
    pub document_title: Option<String>,
    pub section: Option<String>,
    pub adjacent_chunks: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnrichedText(pub String);
```

Update `arcanum-core/src/types/mod.rs`:
```rust
pub mod document;
pub mod enrichment;
pub mod graph;
pub mod operation;
pub mod query;
pub mod tree;
pub use document::*;
pub use enrichment::*;
pub use graph::*;
pub use operation::*;
pub use query::*;
pub use tree::*;
```

- [ ] **Step 3: Run tests**

```bash
cargo test -p arcanum-core
```
Expected: all tests pass.

- [ ] **Step 4: Commit**

```bash
git add arcanum-core/src/
git commit -m "feat(core): add query, graph, tree, operation, and enrichment types"
```

---

### Task 5: Core Traits — DocumentLoader, Preprocessor, Chunker

**Files:**
- Create: `arcanum-core/src/traits/mod.rs`
- Create: `arcanum-core/src/traits/ingestion.rs`

- [ ] **Step 1: Write the failing test**

```rust
// arcanum-core/src/traits/ingestion.rs (test section)
#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::*;

    struct MockLoader;

    #[async_trait::async_trait]
    impl DocumentLoader for MockLoader {
        async fn load(&self, source: &Source) -> crate::Result<RawDocument> {
            Ok(RawDocument {
                id: DocumentId::new(),
                content: b"hello".to_vec(),
                mime_type: "text/plain".to_string(),
                source_uri: source.uri().to_string(),
                metadata: Default::default(),
            })
        }
        fn supports(&self, source: &Source) -> bool {
            matches!(source, Source::File(_))
        }
    }

    #[tokio::test]
    async fn test_loader_trait() {
        let loader = MockLoader;
        let source = Source::File("/tmp/test.txt".into());
        assert!(loader.supports(&source));
        let doc = loader.load(&source).await.unwrap();
        assert_eq!(doc.content, b"hello");
    }
}
```

- [ ] **Step 2: Implement `arcanum-core/src/traits/ingestion.rs`**

```rust
use async_trait::async_trait;
use crate::types::*;
use crate::Result;

/// Where a document originates.
#[derive(Debug, Clone)]
pub enum Source {
    File(std::path::PathBuf),
    Url(String),
    Database { connection_string: String, query: String },
    Raw { content: Vec<u8>, mime_type: String, uri: String },
}

impl Source {
    pub fn uri(&self) -> &str {
        match self {
            Source::File(p) => p.to_str().unwrap_or(""),
            Source::Url(u) => u,
            Source::Database { connection_string, .. } => connection_string,
            Source::Raw { uri, .. } => uri,
        }
    }
}

#[async_trait]
pub trait DocumentLoader: Send + Sync {
    async fn load(&self, source: &Source) -> Result<RawDocument>;
    fn supports(&self, source: &Source) -> bool;
}

#[async_trait]
pub trait Preprocessor: Send + Sync {
    async fn process(&self, doc: RawDocument) -> Result<RawDocument>;
}

#[async_trait]
pub trait Chunker: Send + Sync {
    async fn chunk(&self, doc: &RawDocument) -> Result<Vec<Chunk>>;
}
```

Update `arcanum-core/src/traits/mod.rs`:
```rust
pub mod ingestion;
pub use ingestion::*;
```

Update `arcanum-core/src/lib.rs`:
```rust
pub mod error;
pub mod traits;
pub mod types;
pub use error::{ArcanumError, Result};
```

- [ ] **Step 3: Run tests**

```bash
cargo test -p arcanum-core
```
Expected: all tests pass.

- [ ] **Step 4: Commit**

```bash
git add arcanum-core/src/
git commit -m "feat(core): add DocumentLoader, Preprocessor, Chunker traits"
```

---

### Task 6: Core Traits — TextEnricher, Embedder

**Files:**
- Create: `arcanum-core/src/traits/model.rs`

- [ ] **Step 1: Write the failing test**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::*;

    struct MockEnricher;
    #[async_trait::async_trait]
    impl TextEnricher for MockEnricher {
        async fn enrich(&self, req: EnrichRequest) -> crate::Result<EnrichedText> {
            Ok(EnrichedText(format!("[context] {}", req.text)))
        }
    }

    struct MockEmbedder;
    #[async_trait::async_trait]
    impl Embedder for MockEmbedder {
        async fn embed(&self, texts: Vec<String>) -> crate::Result<Vec<Vector>> {
            Ok(texts.iter().map(|_| Vector(vec![0.1, 0.2, 0.3])).collect())
        }
        fn dimension(&self) -> usize { 3 }
    }

    #[tokio::test]
    async fn test_enricher_context_prefix() {
        let e = MockEnricher;
        let result = e.enrich(EnrichRequest {
            text: "chunk text".to_string(),
            intent: EnrichIntent::ContextPrefix,
            context: None,
        }).await.unwrap();
        assert!(result.0.contains("chunk text"));
    }

    #[tokio::test]
    async fn test_embedder_dimension() {
        let e = MockEmbedder;
        let vecs = e.embed(vec!["a".into(), "b".into()]).await.unwrap();
        assert_eq!(vecs.len(), 2);
        assert_eq!(vecs[0].0.len(), e.dimension());
    }
}
```

- [ ] **Step 2: Implement `arcanum-core/src/traits/model.rs`**

```rust
use async_trait::async_trait;
use crate::types::*;
use crate::Result;

#[async_trait]
pub trait TextEnricher: Send + Sync {
    async fn enrich(&self, request: EnrichRequest) -> Result<EnrichedText>;
}

#[async_trait]
pub trait Embedder: Send + Sync {
    /// Embed a batch of texts into dense vectors.
    async fn embed(&self, texts: Vec<String>) -> Result<Vec<Vector>>;
    fn dimension(&self) -> usize;
}
```

Add to `arcanum-core/src/traits/mod.rs`:
```rust
pub mod ingestion;
pub mod model;
pub use ingestion::*;
pub use model::*;
```

- [ ] **Step 3: Run tests**

```bash
cargo test -p arcanum-core
```
Expected: all tests pass.

- [ ] **Step 4: Commit**

```bash
git add arcanum-core/src/
git commit -m "feat(core): add TextEnricher and Embedder traits"
```

---

### Task 7: Core Traits — VectorStore, GraphStore, TreeStore

**Files:**
- Create: `arcanum-core/src/traits/store.rs`

- [ ] **Step 1: Write the failing test**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    struct InMemoryVectorStore(Mutex<HashMap<String, Vec<IndexedChunk>>>);

    #[async_trait::async_trait]
    impl VectorStore for InMemoryVectorStore {
        async fn upsert(&self, collection: &str, chunks: Vec<IndexedChunk>) -> crate::Result<()> {
            self.0.lock().unwrap().entry(collection.to_string()).or_default().extend(chunks);
            Ok(())
        }
        async fn search(&self, collection: &str, query: &VectorQuery) -> crate::Result<Vec<ScoredChunk>> {
            let store = self.0.lock().unwrap();
            let chunks = store.get(collection).cloned().unwrap_or_default();
            Ok(chunks.into_iter().take(query.top_k).map(|c| ScoredChunk { chunk: c, score: 0.9 }).collect())
        }
        async fn delete(&self, _collection: &str, _ids: &[ChunkId]) -> crate::Result<()> { Ok(()) }
        async fn collection_exists(&self, collection: &str) -> crate::Result<bool> {
            Ok(self.0.lock().unwrap().contains_key(collection))
        }
    }

    #[tokio::test]
    async fn test_vector_store_upsert_and_search() {
        let store = InMemoryVectorStore(Mutex::new(HashMap::new()));
        let chunk = IndexedChunk {
            chunk: Chunk {
                id: ChunkId::new(), text: "hello".into(),
                document_id: DocumentId::new(),
                collection_id: CollectionId("test".into()),
                position: ChunkPosition { start: 0, end: 5, index: 0 },
                metadata: ChunkMetadata::default(),
            },
            vector: Vector(vec![0.1, 0.2]),
            token_vectors: None,
            store_id: "".into(),
        };
        store.upsert("test", vec![chunk]).await.unwrap();
        let results = store.search("test", &VectorQuery { vector: Vector(vec![0.1, 0.2]), top_k: 5, filters: vec![] }).await.unwrap();
        assert_eq!(results.len(), 1);
    }
}
```

- [ ] **Step 2: Implement `arcanum-core/src/traits/store.rs`**

```rust
use async_trait::async_trait;
use crate::types::*;
use crate::Result;

#[derive(Debug, Clone)]
pub struct VectorQuery {
    pub vector: Vector,
    pub top_k: usize,
    pub filters: Vec<MetadataFilter>,
}

#[derive(Debug, Clone)]
pub struct ScoredChunk {
    pub chunk: IndexedChunk,
    pub score: f32,
}

#[async_trait]
pub trait VectorStore: Send + Sync {
    async fn upsert(&self, collection: &str, chunks: Vec<IndexedChunk>) -> Result<()>;
    async fn search(&self, collection: &str, query: &VectorQuery) -> Result<Vec<ScoredChunk>>;
    async fn delete(&self, collection: &str, ids: &[ChunkId]) -> Result<()>;
    async fn collection_exists(&self, collection: &str) -> Result<bool>;
}

#[derive(Debug, Clone)]
pub struct GraphQuery {
    pub entity_name: Option<String>,
    pub entity_type: Option<String>,
    pub max_hops: u32,
    pub relation_filter: Option<String>,
}

#[async_trait]
pub trait GraphStore: Send + Sync {
    async fn upsert_entities(&self, entities: Vec<Entity>) -> Result<()>;
    async fn upsert_relations(&self, relations: Vec<Relation>) -> Result<()>;
    async fn query(&self, q: &GraphQuery) -> Result<Vec<Entity>>;
    async fn get_relations(&self, entity_id: &EntityId) -> Result<Vec<Relation>>;
}

#[async_trait]
pub trait TreeStore: Send + Sync {
    async fn insert_node(&self, collection: &str, node: TreeNode) -> Result<()>;
    async fn get_level(&self, collection: &str, level: u32) -> Result<Vec<TreeNode>>;
    async fn get_children(&self, node_id: &TreeNodeId) -> Result<Vec<TreeNode>>;
}

/// SecretStore: load credentials from the environment.
#[async_trait]
pub trait SecretStore: Send + Sync {
    async fn get(&self, key: &str) -> Result<String>;
    async fn reload(&self) -> Result<()>;
}
```

Add to `arcanum-core/src/traits/mod.rs`:
```rust
pub mod ingestion;
pub mod model;
pub mod store;
pub use ingestion::*;
pub use model::*;
pub use store::*;
```

- [ ] **Step 3: Run tests**

```bash
cargo test -p arcanum-core
```
Expected: all tests pass.

- [ ] **Step 4: Commit**

```bash
git add arcanum-core/src/
git commit -m "feat(core): add VectorStore, GraphStore, TreeStore, SecretStore traits"
```

---

### Task 8: Core Traits — Retriever, Reranker, Evaluator

**Files:**
- Create: `arcanum-core/src/traits/retrieval.rs`

- [ ] **Step 1: Write failing test and implement**

```rust
// arcanum-core/src/traits/retrieval.rs
use async_trait::async_trait;
use crate::types::*;
use crate::Result;

#[async_trait]
pub trait Retriever: Send + Sync {
    async fn retrieve(&self, query: &Query) -> Result<Vec<RetrievedChunk>>;
    fn strategy(&self) -> RetrievalStrategy;
}

#[async_trait]
pub trait Reranker: Send + Sync {
    async fn rerank(&self, query: &Query, chunks: Vec<RetrievedChunk>) -> Result<Vec<RetrievedChunk>>;
}

/// Ground truth entry for evaluation.
#[derive(Debug, Clone)]
pub struct GroundTruth {
    pub query: Query,
    pub relevant_chunk_ids: Vec<ChunkId>,
    pub expected_answer: Option<String>,
}

#[derive(Debug, Clone)]
pub struct EvalMetrics {
    pub hit_rate_at_k: f32,
    pub mrr: f32,
    pub ndcg_at_k: f32,
    pub k: usize,
}

#[async_trait]
pub trait Evaluator: Send + Sync {
    async fn evaluate(
        &self,
        results: &[(Query, Vec<RetrievedChunk>)],
        ground_truths: &[GroundTruth],
    ) -> Result<EvalMetrics>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::*;

    struct PassthroughReranker;
    #[async_trait]
    impl Reranker for PassthroughReranker {
        async fn rerank(&self, _q: &Query, chunks: Vec<RetrievedChunk>) -> Result<Vec<RetrievedChunk>> {
            Ok(chunks)
        }
    }

    #[tokio::test]
    async fn test_reranker_passthrough() {
        let r = PassthroughReranker;
        let q = Query::new("test");
        let result = r.rerank(&q, vec![]).await.unwrap();
        assert!(result.is_empty());
    }
}
```

Add to `arcanum-core/src/traits/mod.rs`:
```rust
pub mod ingestion;
pub mod model;
pub mod retrieval;
pub mod store;
pub use ingestion::*;
pub use model::*;
pub use retrieval::*;
pub use store::*;
```

- [ ] **Step 2: Run and commit**

```bash
cargo test -p arcanum-core
git add arcanum-core/src/
git commit -m "feat(core): add Retriever, Reranker, Evaluator traits"
```

---

### Task 9: ArcanumConfig System

**Files:**
- Create: `arcanum-core/src/config.rs`

- [ ] **Step 1: Write the failing test**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_defaults() {
        let cfg = ArcanumConfig::default();
        assert_eq!(cfg.global.runtime_mode, RuntimeMode::Development);
        assert_eq!(cfg.retrieval.top_k, 5);
    }

    #[test]
    fn test_config_from_env() {
        std::env::set_var("ARCANUM_RUNTIME_MODE", "production");
        let cfg = ArcanumConfig::from_env();
        assert_eq!(cfg.global.runtime_mode, RuntimeMode::Production);
        std::env::remove_var("ARCANUM_RUNTIME_MODE");
    }

    #[test]
    fn test_production_rejects_sqlite() {
        let mut cfg = ArcanumConfig::default();
        cfg.global.runtime_mode = RuntimeMode::Production;
        cfg.storage.metadata_backend = MetadataBackend::Sqlite;
        assert!(cfg.validate().is_err());
    }
}
```

- [ ] **Step 2: Implement `arcanum-core/src/config.rs`**

```rust
use serde::{Deserialize, Serialize};
use crate::{ArcanumError, Result};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum RuntimeMode { Development, Production, Enterprise }
impl Default for RuntimeMode { fn default() -> Self { Self::Development } }

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum MetadataBackend { Sqlite, Postgres }

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GlobalConfig {
    pub runtime_mode: RuntimeMode,
    pub log_level: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    pub metadata_backend: MetadataBackend,
    pub vector_backend: String,
    pub graph_enabled: bool,
    pub tree_enabled: bool,
}
impl Default for StorageConfig {
    fn default() -> Self {
        Self { metadata_backend: MetadataBackend::Sqlite, vector_backend: "lancedb".into(), graph_enabled: false, tree_enabled: false }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetrievalConfig {
    pub top_k: usize,
    pub orchestration_mode: OrchestrationMode,
    pub fusion_strategy: FusionStrategy,
    pub query_cache_enabled: bool,
}
impl Default for RetrievalConfig {
    fn default() -> Self {
        Self { top_k: 5, orchestration_mode: OrchestrationMode::ParallelFusion, fusion_strategy: FusionStrategy::Rrf, query_cache_enabled: false }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum OrchestrationMode { Static, QueryClassified, ParallelFusion }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FusionStrategy { Rrf, Weighted(Vec<(String, f32)>), Learned }

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ArcanumConfig {
    pub global: GlobalConfig,
    pub storage: StorageConfig,
    pub retrieval: RetrievalConfig,
}

impl ArcanumConfig {
    pub fn from_env() -> Self {
        let mut cfg = Self::default();
        if let Ok(mode) = std::env::var("ARCANUM_RUNTIME_MODE") {
            cfg.global.runtime_mode = match mode.to_lowercase().as_str() {
                "production" => RuntimeMode::Production,
                "enterprise" => RuntimeMode::Enterprise,
                _ => RuntimeMode::Development,
            };
        }
        cfg
    }

    pub fn validate(&self) -> Result<()> {
        if self.global.runtime_mode != RuntimeMode::Development
            && self.storage.metadata_backend == MetadataBackend::Sqlite
        {
            return Err(ArcanumError::Config(
                "SQLite is not allowed in production or enterprise mode. Use PostgreSQL.".into(),
            ));
        }
        Ok(())
    }
}
```

Update `arcanum-core/src/lib.rs`:
```rust
pub mod config;
pub mod error;
pub mod traits;
pub mod types;
pub use config::ArcanumConfig;
pub use error::{ArcanumError, Result};
```

- [ ] **Step 3: Run tests**

```bash
cargo test -p arcanum-core
```
Expected: all tests pass (including SQLite-in-production rejection).

- [ ] **Step 4: Commit**

```bash
git add arcanum-core/src/
git commit -m "feat(core): add ArcanumConfig with layered config and production validation"
```

---

## Phase 1 Complete ✓

`arcanum-core` is fully implemented and tested. Every other crate depends on this foundation. Verify:

```bash
cargo test -p arcanum-core --verbose
```

All tests should pass. Proceed to **Part 2** (arcanum-vector, arcanum-graph, arcanum-tree, arcanum-models).
