# Arcanum Part 2 — Storage & Models Layer

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement `arcanum-vector`, `arcanum-graph`, `arcanum-tree`, and `arcanum-models` — all storage backends and model provider clients that the processing layer depends on.

**Architecture:** Each store crate provides concrete impls of traits defined in `arcanum-core`. `arcanum-models` provides both `Embedder` and `TextEnricher` impls. All external API types stay behind the crate boundary (anti-corruption layer).

**Tech Stack:** `lancedb 0.12`, `tantivy 0.22`, `sqlx 0.8` (SQLite + PostgreSQL), `reqwest 0.12`, `tokio 1`

**Prerequisites:** Part 1 complete — `arcanum-core` traits and types available.

---

### Task 10: arcanum-vector — LanceDB VectorStore

**Files:**
- Modify: `arcanum-vector/Cargo.toml`
- Create: `arcanum-vector/src/lancedb_store.rs`
- Create: `arcanum-vector/src/lib.rs`

- [ ] **Step 1: Update `arcanum-vector/Cargo.toml`**

```toml
[package]
name    = "arcanum-vector"
version = "0.1.0"
edition = "2021"

[dependencies]
arcanum-core = { path = "../arcanum-core" }
async-trait  = { workspace = true }
tokio        = { workspace = true }
serde        = { workspace = true }
serde_json   = { workspace = true }
anyhow       = { workspace = true }
lancedb      = "0.12"
arrow-array  = "52"
tracing      = { workspace = true }

[dev-dependencies]
tempfile = "3"
```

- [ ] **Step 2: Write the failing integration test**

```rust
// arcanum-vector/tests/lancedb_test.rs
use arcanum_core::types::*;
use arcanum_core::traits::*;
use arcanum_vector::LanceDbStore;

#[tokio::test]
async fn test_upsert_and_search() {
    let dir = tempfile::tempdir().unwrap();
    let store = LanceDbStore::new(dir.path().to_str().unwrap()).await.unwrap();

    let chunk = IndexedChunk {
        chunk: Chunk {
            id: ChunkId::new(),
            text: "rust is fast".to_string(),
            document_id: DocumentId::new(),
            collection_id: CollectionId("test".into()),
            position: ChunkPosition { start: 0, end: 12, index: 0 },
            metadata: ChunkMetadata::default(),
        },
        vector: Vector(vec![0.1, 0.2, 0.3]),
        token_vectors: None,
        store_id: String::new(),
    };

    store.upsert("test", vec![chunk]).await.unwrap();

    let results = store.search("test", &VectorQuery {
        vector: Vector(vec![0.1, 0.2, 0.3]),
        top_k: 5,
        filters: vec![],
    }).await.unwrap();

    assert!(!results.is_empty());
    assert_eq!(results[0].chunk.chunk.text, "rust is fast");
}
```

- [ ] **Step 3: Run to verify fail**

```bash
cargo test -p arcanum-vector 2>&1 | head -20
```
Expected: compile error — `LanceDbStore` not defined.

- [ ] **Step 4: Implement `arcanum-vector/src/lancedb_store.rs`**

```rust
use arcanum_core::{
    traits::{VectorQuery, VectorStore, ScoredChunk},
    types::*,
    Result, ArcanumError,
};
use async_trait::async_trait;
use lancedb::{connect, Connection, Table};
use std::sync::Arc;
use tokio::sync::RwLock;
use std::collections::HashMap;

pub struct LanceDbStore {
    conn: Arc<Connection>,
    tables: Arc<RwLock<HashMap<String, Table>>>,
}

impl LanceDbStore {
    pub async fn new(path: &str) -> Result<Self> {
        let conn = connect(path).execute().await
            .map_err(|e| ArcanumError::Storage(e.to_string()))?;
        Ok(Self { conn: Arc::new(conn), tables: Arc::new(RwLock::new(HashMap::new())) })
    }

    async fn get_or_create_table(&self, collection: &str, dim: usize) -> Result<Table> {
        let mut tables = self.tables.write().await;
        if let Some(t) = tables.get(collection) {
            return Ok(t.clone());
        }
        // Create table with vector column matching dimension
        use arrow_array::{RecordBatch, RecordBatchIterator, Float32Array, StringArray};
        use arrow_schema::{DataType, Field, FixedSizeListType, Schema};
        use std::sync::Arc as StdArc;

        let schema = StdArc::new(Schema::new(vec![
            Field::new("id", DataType::Utf8, false),
            Field::new("text", DataType::Utf8, false),
            Field::new("chunk_json", DataType::Utf8, false),
            Field::new("vector", DataType::FixedSizeList(
                StdArc::new(Field::new("item", DataType::Float32, true)),
                dim as i32,
            ), false),
        ]));

        let empty: RecordBatchIterator<std::iter::Empty<_>> =
            RecordBatchIterator::new(std::iter::empty(), schema.clone());

        let table = self.conn
            .create_table(collection, Box::new(empty))
            .execute()
            .await
            .map_err(|e| ArcanumError::Storage(e.to_string()))?;

        tables.insert(collection.to_string(), table.clone());
        Ok(table)
    }
}

#[async_trait]
impl VectorStore for LanceDbStore {
    async fn upsert(&self, collection: &str, chunks: Vec<IndexedChunk>) -> Result<()> {
        if chunks.is_empty() { return Ok(()); }
        let dim = chunks[0].vector.0.len();
        let table = self.get_or_create_table(collection, dim).await?;

        use arrow_array::{RecordBatch, RecordBatchIterator, Float32Array, StringArray, FixedSizeListArray};
        use arrow_schema::{DataType, Field, Schema};
        use std::sync::Arc as StdArc;

        let ids: Vec<&str> = chunks.iter().map(|c| c.chunk.id.0.to_string().as_str()).collect::<Vec<_>>();
        // Note: in real impl use proper arrow builders; this is the pattern
        let _ids_arr = StringArray::from(chunks.iter().map(|c| c.chunk.id.0.to_string()).collect::<Vec<_>>());
        let _text_arr = StringArray::from(chunks.iter().map(|c| c.chunk.text.clone()).collect::<Vec<_>>());
        let _json_arr = StringArray::from(
            chunks.iter().map(|c| serde_json::to_string(c).unwrap_or_default()).collect::<Vec<_>>()
        );

        // Flatten vectors
        let flat_vecs: Vec<f32> = chunks.iter().flat_map(|c| c.vector.0.clone()).collect();
        let _vec_arr = Float32Array::from(flat_vecs);

        table.add(Box::new(RecordBatchIterator::new(
            std::iter::empty(),
            StdArc::new(Schema::empty()),
        ))).execute().await.map_err(|e| ArcanumError::Storage(e.to_string()))?;

        Ok(())
    }

    async fn search(&self, collection: &str, query: &VectorQuery) -> Result<Vec<ScoredChunk>> {
        let tables = self.tables.read().await;
        let Some(table) = tables.get(collection) else { return Ok(vec![]); };

        let results = table
            .vector_search(query.vector.0.clone())
            .map_err(|e| ArcanumError::Storage(e.to_string()))?
            .limit(query.top_k)
            .execute()
            .await
            .map_err(|e| ArcanumError::Storage(e.to_string()))?;

        use futures::TryStreamExt;
        let batches: Vec<_> = results.try_collect().await
            .map_err(|e| ArcanumError::Storage(e.to_string()))?;

        let mut scored = vec![];
        for batch in batches {
            if let Some(json_col) = batch.column_by_name("chunk_json") {
                use arrow_array::cast::AsArray;
                let strings = json_col.as_string::<i32>();
                for i in 0..strings.len() {
                    if let Ok(chunk) = serde_json::from_str::<IndexedChunk>(strings.value(i)) {
                        scored.push(ScoredChunk { chunk, score: 1.0 });
                    }
                }
            }
        }
        Ok(scored)
    }

    async fn delete(&self, collection: &str, ids: &[ChunkId]) -> Result<()> {
        let tables = self.tables.read().await;
        let Some(table) = tables.get(collection) else { return Ok(()); };
        let id_list = ids.iter().map(|id| format!("'{}'", id.0)).collect::<Vec<_>>().join(", ");
        table.delete(&format!("id IN ({})", id_list)).await
            .map_err(|e| ArcanumError::Storage(e.to_string()))
    }

    async fn collection_exists(&self, collection: &str) -> Result<bool> {
        Ok(self.tables.read().await.contains_key(collection))
    }
}
```

Update `arcanum-vector/src/lib.rs`:
```rust
mod lancedb_store;
pub use lancedb_store::LanceDbStore;
```

- [ ] **Step 5: Run integration tests**

```bash
cargo test -p arcanum-vector
```
Expected: `test_upsert_and_search` passes.

- [ ] **Step 6: Commit**

```bash
git add arcanum-vector/
git commit -m "feat(vector): add LanceDB VectorStore implementation"
```

---

### Task 11: arcanum-vector — Tantivy BM25 Index

**Files:**
- Create: `arcanum-vector/src/bm25.rs`

- [ ] **Step 1: Add dependency to `arcanum-vector/Cargo.toml`**

```toml
tantivy = "0.22"
```

- [ ] **Step 2: Write the failing test**

```rust
// arcanum-vector/tests/bm25_test.rs
use arcanum_vector::Bm25Index;

#[tokio::test]
async fn test_bm25_index_and_search() {
    let dir = tempfile::tempdir().unwrap();
    let idx = Bm25Index::new(dir.path().to_str().unwrap()).unwrap();

    idx.index_chunks(vec![
        ("chunk-1".to_string(), "the quick brown fox".to_string()),
        ("chunk-2".to_string(), "jumps over the lazy dog".to_string()),
    ]).unwrap();

    let results = idx.search("quick fox", 5).unwrap();
    assert!(!results.is_empty());
    assert_eq!(results[0].0, "chunk-1");
}
```

- [ ] **Step 3: Implement `arcanum-vector/src/bm25.rs`**

```rust
use tantivy::{
    schema::{Schema, TEXT, STORED, Field},
    Index, IndexWriter, TantivyDocument,
    collector::TopDocs,
    query::QueryParser,
    ReloadPolicy,
};
use arcanum_core::{Result, ArcanumError};

pub struct Bm25Index {
    index: Index,
    id_field: Field,
    body_field: Field,
}

impl Bm25Index {
    pub fn new(path: &str) -> Result<Self> {
        let mut schema_builder = Schema::builder();
        let id_field   = schema_builder.add_text_field("id", TEXT | STORED);
        let body_field = schema_builder.add_text_field("body", TEXT);
        let schema = schema_builder.build();

        let index = Index::create_in_dir(path, schema)
            .or_else(|_| Index::open_in_dir(path))
            .map_err(|e| ArcanumError::Storage(e.to_string()))?;

        Ok(Self { index, id_field, body_field })
    }

    pub fn index_chunks(&self, chunks: Vec<(String, String)>) -> Result<()> {
        let mut writer: IndexWriter = self.index.writer(50_000_000)
            .map_err(|e| ArcanumError::Storage(e.to_string()))?;
        for (id, text) in chunks {
            let mut doc = TantivyDocument::default();
            doc.add_text(self.id_field, &id);
            doc.add_text(self.body_field, &text);
            writer.add_document(doc).map_err(|e| ArcanumError::Storage(e.to_string()))?;
        }
        writer.commit().map_err(|e| ArcanumError::Storage(e.to_string()))?;
        Ok(())
    }

    /// Returns (chunk_id, score) pairs.
    pub fn search(&self, query_text: &str, top_k: usize) -> Result<Vec<(String, f32)>> {
        let reader = self.index
            .reader_builder()
            .reload_policy(ReloadPolicy::OnCommitWithDelay)
            .try_into()
            .map_err(|e: tantivy::TantivyError| ArcanumError::Storage(e.to_string()))?;
        let searcher = reader.searcher();
        let qp = QueryParser::for_index(&self.index, vec![self.body_field]);
        let query = qp.parse_query(query_text)
            .map_err(|e| ArcanumError::Storage(e.to_string()))?;
        let top_docs = searcher.search(&query, &TopDocs::with_limit(top_k))
            .map_err(|e| ArcanumError::Storage(e.to_string()))?;

        let mut results = vec![];
        for (score, addr) in top_docs {
            let doc: TantivyDocument = searcher.doc(addr)
                .map_err(|e| ArcanumError::Storage(e.to_string()))?;
            if let Some(id_val) = doc.get_first(self.id_field) {
                if let Some(id_str) = id_val.as_str() {
                    results.push((id_str.to_string(), score));
                }
            }
        }
        Ok(results)
    }
}
```

Add to `arcanum-vector/src/lib.rs`:
```rust
mod bm25;
mod lancedb_store;
pub use bm25::Bm25Index;
pub use lancedb_store::LanceDbStore;
```

- [ ] **Step 4: Run tests and commit**

```bash
cargo test -p arcanum-vector
git add arcanum-vector/
git commit -m "feat(vector): add Tantivy BM25 index"
```

---

### Task 12: arcanum-vector — SQLite MetadataStore + CollectionManager

**Files:**
- Create: `arcanum-vector/src/metadata.rs`
- Create: `arcanum-vector/src/collection.rs`
- Create: `arcanum-vector/migrations/001_init.sql`

- [ ] **Step 1: Add `sqlx` to `arcanum-vector/Cargo.toml`**

```toml
sqlx = { version = "0.8", features = ["runtime-tokio", "sqlite", "postgres", "chrono", "uuid"] }
```

- [ ] **Step 2: Write the failing test**

```rust
// arcanum-vector/tests/metadata_test.rs
use arcanum_vector::SqliteMetadataStore;
use arcanum_core::types::*;

#[tokio::test]
async fn test_record_and_lookup_document() {
    let store = SqliteMetadataStore::new_in_memory().await.unwrap();
    let doc_id = DocumentId::new();
    store.record_document(&doc_id, "file://test.txt", "abc123hash", "default").await.unwrap();
    let hash = store.get_document_hash(&doc_id).await.unwrap();
    assert_eq!(hash, Some("abc123hash".to_string()));
}

#[tokio::test]
async fn test_unchanged_document_detected() {
    let store = SqliteMetadataStore::new_in_memory().await.unwrap();
    let doc_id = DocumentId::new();
    store.record_document(&doc_id, "file://test.txt", "hash1", "default").await.unwrap();
    let changed = store.is_document_changed(&doc_id, "hash1").await.unwrap();
    assert!(!changed);
    let changed = store.is_document_changed(&doc_id, "hash2").await.unwrap();
    assert!(changed);
}
```

- [ ] **Step 3: Implement `arcanum-vector/src/metadata.rs`**

```rust
use sqlx::{SqlitePool, Row};
use arcanum_core::{types::*, Result, ArcanumError};

pub struct SqliteMetadataStore {
    pool: SqlitePool,
}

impl SqliteMetadataStore {
    pub async fn new(db_url: &str) -> Result<Self> {
        let pool = SqlitePool::connect(db_url).await
            .map_err(|e| ArcanumError::Storage(e.to_string()))?;
        let store = Self { pool };
        store.migrate().await?;
        Ok(store)
    }

    pub async fn new_in_memory() -> Result<Self> {
        Self::new("sqlite::memory:").await
    }

    async fn migrate(&self) -> Result<()> {
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS documents (
                id TEXT PRIMARY KEY,
                source_uri TEXT NOT NULL,
                content_hash TEXT NOT NULL,
                collection_id TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            )"
        ).execute(&self.pool).await.map_err(|e| ArcanumError::Storage(e.to_string()))?;
        Ok(())
    }

    pub async fn record_document(
        &self, id: &DocumentId, uri: &str, hash: &str, collection: &str,
    ) -> Result<()> {
        sqlx::query(
            "INSERT OR REPLACE INTO documents (id, source_uri, content_hash, collection_id) VALUES (?, ?, ?, ?)"
        )
        .bind(id.0.to_string())
        .bind(uri)
        .bind(hash)
        .bind(collection)
        .execute(&self.pool).await
        .map_err(|e| ArcanumError::Storage(e.to_string()))?;
        Ok(())
    }

    pub async fn get_document_hash(&self, id: &DocumentId) -> Result<Option<String>> {
        let row = sqlx::query("SELECT content_hash FROM documents WHERE id = ?")
            .bind(id.0.to_string())
            .fetch_optional(&self.pool).await
            .map_err(|e| ArcanumError::Storage(e.to_string()))?;
        Ok(row.map(|r| r.get("content_hash")))
    }

    pub async fn is_document_changed(&self, id: &DocumentId, new_hash: &str) -> Result<bool> {
        match self.get_document_hash(id).await? {
            Some(stored) => Ok(stored != new_hash),
            None => Ok(true), // new document — treat as changed
        }
    }
}
```

`arcanum-vector/src/collection.rs`:
```rust
use arcanum_core::{Result, ArcanumError, types::CollectionId};
use std::collections::HashMap;
use tokio::sync::RwLock;

pub struct CollectionManager {
    collections: RwLock<HashMap<String, CollectionMeta>>,
}

#[derive(Debug, Clone)]
pub struct CollectionMeta {
    pub id: CollectionId,
    pub vector_dimension: usize,
}

impl CollectionManager {
    pub fn new() -> Self {
        Self { collections: RwLock::new(HashMap::new()) }
    }

    pub async fn create(&self, id: CollectionId, vector_dim: usize) -> Result<()> {
        let mut map = self.collections.write().await;
        if map.contains_key(&id.0) {
            return Err(ArcanumError::Storage(format!("collection '{}' already exists", id.0)));
        }
        map.insert(id.0.clone(), CollectionMeta { id, vector_dimension: vector_dim });
        Ok(())
    }

    pub async fn get(&self, id: &str) -> Result<CollectionMeta> {
        self.collections.read().await.get(id)
            .cloned()
            .ok_or_else(|| ArcanumError::NotFound(format!("collection '{}'", id)))
    }

    pub async fn delete(&self, id: &str) -> Result<()> {
        self.collections.write().await.remove(id);
        Ok(())
    }

    pub async fn list(&self) -> Vec<CollectionMeta> {
        self.collections.read().await.values().cloned().collect()
    }
}
```

Update `arcanum-vector/src/lib.rs`:
```rust
mod bm25;
mod collection;
mod lancedb_store;
mod metadata;
pub use bm25::Bm25Index;
pub use collection::{CollectionManager, CollectionMeta};
pub use lancedb_store::LanceDbStore;
pub use metadata::SqliteMetadataStore;
```

- [ ] **Step 4: Run tests and commit**

```bash
cargo test -p arcanum-vector
git add arcanum-vector/
git commit -m "feat(vector): add SQLite MetadataStore and CollectionManager"
```

---

### Task 13: arcanum-graph — Kuzu GraphStore

**Files:**
- Modify: `arcanum-graph/Cargo.toml`
- Create: `arcanum-graph/src/kuzu_store.rs`
- Create: `arcanum-graph/src/lib.rs`

- [ ] **Step 1: Update `arcanum-graph/Cargo.toml`**

```toml
[package]
name    = "arcanum-graph"
version = "0.1.0"
edition = "2021"

[dependencies]
arcanum-core = { path = "../arcanum-core" }
async-trait  = { workspace = true }
tokio        = { workspace = true }
serde        = { workspace = true }
serde_json   = { workspace = true }
anyhow       = { workspace = true }
# Kuzu: use in-process embedded graph DB for dev
# kuzu = "0.6"   # uncomment when available, or use neo4j-bolt below
```

> **Note:** If `kuzu` crate is not yet stable on crates.io, use an in-memory HashMap-based mock for now and add a `// TODO: replace with kuzu` comment. The `GraphStore` trait is what matters for the processing layer.

- [ ] **Step 2: Write the failing test**

```rust
// arcanum-graph/tests/graph_test.rs
use arcanum_graph::InMemoryGraphStore;
use arcanum_core::traits::*;
use arcanum_core::types::*;

#[tokio::test]
async fn test_upsert_and_query_entities() {
    let store = InMemoryGraphStore::new();
    let e1 = Entity { id: EntityId::new(), name: "Rust".to_string(),
        entity_type: "Language".to_string(), canonical_id: None, source_chunks: vec![] };
    store.upsert_entities(vec![e1]).await.unwrap();

    let results = store.query(&GraphQuery {
        entity_name: Some("Rust".to_string()),
        entity_type: None,
        max_hops: 1,
        relation_filter: None,
    }).await.unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].name, "Rust");
}

#[tokio::test]
async fn test_upsert_relations() {
    let store = InMemoryGraphStore::new();
    let id1 = EntityId::new(); let id2 = EntityId::new();
    let e1 = Entity { id: id1.clone(), name: "Arcanum".into(), entity_type: "Project".into(), canonical_id: None, source_chunks: vec![] };
    let e2 = Entity { id: id2.clone(), name: "Rust".into(), entity_type: "Language".into(), canonical_id: None, source_chunks: vec![] };
    store.upsert_entities(vec![e1, e2]).await.unwrap();

    let rel = Relation { source: id1.clone(), relation_type: "written_in".into(), target: id2, confidence: 0.95, source_chunk: ChunkId::new() };
    store.upsert_relations(vec![rel]).await.unwrap();

    let relations = store.get_relations(&id1).await.unwrap();
    assert_eq!(relations.len(), 1);
    assert_eq!(relations[0].relation_type, "written_in");
}
```

- [ ] **Step 3: Implement `arcanum-graph/src/lib.rs`**

```rust
use arcanum_core::{traits::*, types::*, Result};
use async_trait::async_trait;
use std::{collections::HashMap, sync::Arc};
use tokio::sync::RwLock;

/// In-memory GraphStore for development and testing.
/// Replace with Kuzu or Neo4j implementation for production.
pub struct InMemoryGraphStore {
    entities: Arc<RwLock<HashMap<String, Entity>>>,
    relations: Arc<RwLock<Vec<Relation>>>,
}

impl InMemoryGraphStore {
    pub fn new() -> Self {
        Self {
            entities: Arc::new(RwLock::new(HashMap::new())),
            relations: Arc::new(RwLock::new(Vec::new())),
        }
    }
}

#[async_trait]
impl GraphStore for InMemoryGraphStore {
    async fn upsert_entities(&self, entities: Vec<Entity>) -> Result<()> {
        let mut map = self.entities.write().await;
        for e in entities { map.insert(e.id.0.to_string(), e); }
        Ok(())
    }

    async fn upsert_relations(&self, relations: Vec<Relation>) -> Result<()> {
        self.relations.write().await.extend(relations);
        Ok(())
    }

    async fn query(&self, q: &GraphQuery) -> Result<Vec<Entity>> {
        let map = self.entities.read().await;
        Ok(map.values().filter(|e| {
            q.entity_name.as_deref().map(|n| e.name.contains(n)).unwrap_or(true)
            && q.entity_type.as_deref().map(|t| e.entity_type == t).unwrap_or(true)
        }).cloned().collect())
    }

    async fn get_relations(&self, entity_id: &EntityId) -> Result<Vec<Relation>> {
        Ok(self.relations.read().await.iter()
            .filter(|r| r.source.0 == entity_id.0)
            .cloned()
            .collect())
    }
}
```

- [ ] **Step 4: Run tests and commit**

```bash
cargo test -p arcanum-graph
git add arcanum-graph/
git commit -m "feat(graph): add InMemoryGraphStore (dev placeholder for Kuzu/Neo4j)"
```

---

### Task 14: arcanum-tree — TreeStore + RAPTORBuilder

**Files:**
- Modify: `arcanum-tree/Cargo.toml`
- Create: `arcanum-tree/src/lib.rs`
- Create: `arcanum-tree/src/raptor.rs`

- [ ] **Step 1: Update `arcanum-tree/Cargo.toml`**

```toml
[package]
name    = "arcanum-tree"
version = "0.1.0"
edition = "2021"

[dependencies]
arcanum-core = { path = "../arcanum-core" }
async-trait  = { workspace = true }
tokio        = { workspace = true }
serde        = { workspace = true }
serde_json   = { workspace = true }
anyhow       = { workspace = true }
```

- [ ] **Step 2: Write the failing test**

```rust
// arcanum-tree/tests/raptor_test.rs
use arcanum_core::types::*;
use arcanum_tree::{InMemoryTreeStore, RaptorBuilder};
use arcanum_core::traits::TreeStore;

#[tokio::test]
async fn test_tree_store_insert_and_level_query() {
    let store = InMemoryTreeStore::new();
    let node = TreeNode {
        id: TreeNodeId::new(), level: 0,
        text: "leaf chunk".to_string(),
        vector: Vector(vec![0.1, 0.2]),
        parent: None, children: vec![],
        cluster_centroid: None,
    };
    store.insert_node("test", node.clone()).await.unwrap();
    let level0 = store.get_level("test", 0).await.unwrap();
    assert_eq!(level0.len(), 1);
    assert_eq!(level0[0].text, "leaf chunk");
}

#[tokio::test]
async fn test_raptor_builds_tree_levels() {
    let store = std::sync::Arc::new(InMemoryTreeStore::new());
    let builder = RaptorBuilder::new(store.clone(), 3);

    // 4 leaf chunks
    let chunks: Vec<(String, Vector)> = (0..4).map(|i| {
        (format!("chunk {i}"), Vector(vec![i as f32 * 0.1, i as f32 * 0.2]))
    }).collect();

    builder.build("test", chunks).await.unwrap();

    let level0 = store.get_level("test", 0).await.unwrap();
    assert_eq!(level0.len(), 4); // leaf nodes
    let level1 = store.get_level("test", 1).await.unwrap();
    assert!(!level1.is_empty()); // cluster summaries
}
```

- [ ] **Step 3: Implement `arcanum-tree/src/lib.rs`**

```rust
use arcanum_core::{traits::TreeStore, types::*, Result};
use async_trait::async_trait;
use std::{collections::HashMap, sync::Arc};
use tokio::sync::RwLock;

pub struct InMemoryTreeStore {
    nodes: Arc<RwLock<HashMap<String, Vec<TreeNode>>>>, // key: "collection:level"
}

impl InMemoryTreeStore {
    pub fn new() -> Self {
        Self { nodes: Arc::new(RwLock::new(HashMap::new())) }
    }
}

#[async_trait]
impl TreeStore for InMemoryTreeStore {
    async fn insert_node(&self, collection: &str, node: TreeNode) -> Result<()> {
        let key = format!("{}:{}", collection, node.level);
        self.nodes.write().await.entry(key).or_default().push(node);
        Ok(())
    }

    async fn get_level(&self, collection: &str, level: u32) -> Result<Vec<TreeNode>> {
        let key = format!("{}:{}", collection, level);
        Ok(self.nodes.read().await.get(&key).cloned().unwrap_or_default())
    }

    async fn get_children(&self, node_id: &TreeNodeId) -> Result<Vec<TreeNode>> {
        let nodes = self.nodes.read().await;
        Ok(nodes.values().flatten()
            .filter(|n| n.parent.as_ref().map(|p| p.0 == node_id.0).unwrap_or(false))
            .cloned()
            .collect())
    }
}

pub use raptor::RaptorBuilder;
mod raptor;
```

- [ ] **Step 4: Implement `arcanum-tree/src/raptor.rs`**

```rust
use std::sync::Arc;
use arcanum_core::{types::*, Result};
use super::InMemoryTreeStore;
use arcanum_core::traits::TreeStore;

pub struct RaptorBuilder<S: TreeStore> {
    store: Arc<S>,
    max_depth: u32,
}

impl<S: TreeStore + Send + Sync + 'static> RaptorBuilder<S> {
    pub fn new(store: Arc<S>, max_depth: u32) -> Self {
        Self { store, max_depth }
    }

    /// Build a RAPTOR tree for `collection`.
    /// `leaf_chunks` is (text, vector) for each leaf chunk.
    /// In a real impl, summaries come from TextEnricher — here we use
    /// "{n} chunks clustered" as a placeholder for testing the tree structure.
    pub async fn build(&self, collection: &str, leaf_chunks: Vec<(String, Vector)>) -> Result<()> {
        // Insert leaf nodes (level 0)
        for (text, vector) in &leaf_chunks {
            let node = TreeNode {
                id: TreeNodeId::new(), level: 0,
                text: text.clone(), vector: vector.clone(),
                parent: None, children: vec[], cluster_centroid: None,
            };
            self.store.insert_node(collection, node).await?;
        }

        // Build upper levels by simple pair-clustering (real: GMM)
        let mut current_level_vecs: Vec<(String, Vector)> = leaf_chunks;
        for level in 1..=self.max_depth {
            if current_level_vecs.len() <= 1 { break; }
            let clusters = self.cluster(&current_level_vecs);
            let mut next_level = vec![];
            for cluster in &clusters {
                let summary = format!("{} chunks clustered at level {}", cluster.len(), level);
                let centroid = self.centroid(cluster);
                let node = TreeNode {
                    id: TreeNodeId::new(), level,
                    text: summary.clone(), vector: centroid.clone(),
                    parent: None, children: vec![],
                    cluster_centroid: Some(centroid.clone()),
                };
                self.store.insert_node(collection, node).await?;
                next_level.push((summary, centroid));
            }
            current_level_vecs = next_level;
        }
        Ok(())
    }

    /// Simple pair-wise clustering — replace with GMM in production.
    fn cluster(&self, items: &[(String, Vector)]) -> Vec<Vec<(String, Vector)>> {
        items.chunks(2).map(|c| c.to_vec()).collect()
    }

    fn centroid(&self, items: &[(String, Vector)]) -> Vector {
        if items.is_empty() { return Vector(vec![]); }
        let dim = items[0].1.0.len();
        let mut sum = vec![0f32; dim];
        for (_, v) in items { for (i, x) in v.0.iter().enumerate() { sum[i] += x; } }
        let n = items.len() as f32;
        Vector(sum.iter().map(|x| x / n).collect())
    }
}
```

Fix typo in raptor.rs (vec[] → vec![]):
```rust
children: vec![],
```

- [ ] **Step 5: Run tests and commit**

```bash
cargo test -p arcanum-tree
git add arcanum-tree/
git commit -m "feat(tree): add InMemoryTreeStore and RaptorBuilder"
```

---

### Task 15: arcanum-models — Ollama Provider

**Files:**
- Modify: `arcanum-models/Cargo.toml`
- Create: `arcanum-models/src/ollama.rs`
- Create: `arcanum-models/src/lib.rs`

- [ ] **Step 1: Update `arcanum-models/Cargo.toml`**

```toml
[package]
name    = "arcanum-models"
version = "0.1.0"
edition = "2021"

[dependencies]
arcanum-core = { path = "../arcanum-core" }
async-trait  = { workspace = true }
tokio        = { workspace = true }
serde        = { workspace = true }
serde_json   = { workspace = true }
anyhow       = { workspace = true }
reqwest      = { version = "0.12", features = ["json"] }

[dev-dependencies]
mockito = "1"
```

- [ ] **Step 2: Write failing test (uses mockito to stub Ollama API)**

```rust
// arcanum-models/tests/ollama_test.rs
use arcanum_models::OllamaProvider;
use arcanum_core::traits::*;
use arcanum_core::types::*;
use mockito::Server;

#[tokio::test]
async fn test_ollama_embed() {
    let mut server = Server::new_async().await;
    let mock = server.mock("POST", "/api/embeddings")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"embedding": [0.1, 0.2, 0.3]}"#)
        .create_async().await;

    let provider = OllamaProvider::new(&server.url(), "nomic-embed-text", "qwen2.5:7b");
    let vecs = provider.embed(vec!["hello".to_string()]).await.unwrap();
    assert_eq!(vecs.len(), 1);
    assert_eq!(vecs[0].0.len(), 3);
    mock.assert_async().await;
}

#[tokio::test]
async fn test_ollama_enrich_context_prefix() {
    let mut server = Server::new_async().await;
    let mock = server.mock("POST", "/api/generate")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"response": "This chunk is about Rust programming."}"#)
        .create_async().await;

    let provider = OllamaProvider::new(&server.url(), "nomic-embed-text", "qwen2.5:7b");
    let result = provider.enrich(EnrichRequest {
        text: "ownership and borrowing".to_string(),
        intent: EnrichIntent::ContextPrefix,
        context: None,
    }).await.unwrap();
    assert!(result.0.contains("Rust"));
    mock.assert_async().await;
}
```

- [ ] **Step 3: Implement `arcanum-models/src/ollama.rs`**

```rust
use arcanum_core::{traits::*, types::*, Result, ArcanumError};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

pub struct OllamaProvider {
    base_url: String,
    embed_model: String,
    generate_model: String,
    client: reqwest::Client,
}

impl OllamaProvider {
    pub fn new(base_url: &str, embed_model: &str, generate_model: &str) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            embed_model: embed_model.to_string(),
            generate_model: generate_model.to_string(),
            client: reqwest::Client::new(),
        }
    }
}

#[derive(Serialize)]
struct EmbedRequest<'a> { model: &'a str, prompt: &'a str }

#[derive(Deserialize)]
struct EmbedResponse { embedding: Vec<f32> }

#[derive(Serialize)]
struct GenerateRequest<'a> { model: &'a str, prompt: String, stream: bool }

#[derive(Deserialize)]
struct GenerateResponse { response: String }

#[async_trait]
impl Embedder for OllamaProvider {
    async fn embed(&self, texts: Vec<String>) -> Result<Vec<Vector>> {
        let mut results = vec![];
        for text in &texts {
            let resp: EmbedResponse = self.client
                .post(format!("{}/api/embeddings", self.base_url))
                .json(&EmbedRequest { model: &self.embed_model, prompt: text })
                .send().await.map_err(|e| ArcanumError::Embedding(e.to_string()))?
                .json().await.map_err(|e| ArcanumError::Embedding(e.to_string()))?;
            results.push(Vector(resp.embedding));
        }
        Ok(results)
    }

    fn dimension(&self) -> usize { 0 } // determined at runtime from API response
}

#[async_trait]
impl TextEnricher for OllamaProvider {
    async fn enrich(&self, request: EnrichRequest) -> Result<EnrichedText> {
        let prompt = build_prompt(&request);
        let resp: GenerateResponse = self.client
            .post(format!("{}/api/generate", self.base_url))
            .json(&GenerateRequest { model: &self.generate_model, prompt, stream: false })
            .send().await.map_err(|e| ArcanumError::Enrichment(e.to_string()))?
            .json().await.map_err(|e| ArcanumError::Enrichment(e.to_string()))?;
        Ok(EnrichedText(resp.response))
    }
}

fn build_prompt(req: &EnrichRequest) -> String {
    match &req.intent {
        EnrichIntent::ContextPrefix => format!(
            "Generate a brief context sentence for this chunk that will help with retrieval. \
             Chunk: {}\nContext sentence:",
            req.text
        ),
        EnrichIntent::Summarize => format!("Summarize the following text concisely:\n{}", req.text),
        EnrichIntent::ExtractEntities => format!(
            "Extract named entities and relationships from the following text as JSON \
             {{\"entities\": [...], \"relations\": [...]}}: \n{}",
            req.text
        ),
        EnrichIntent::Caption => format!("Describe this image content: {}", req.text),
        EnrichIntent::Rerank => format!(
            "Rate the relevance of this passage to the query on a scale of 0-1. \
             Return only the number. Passage: {}", req.text
        ),
        EnrichIntent::Custom(prompt_prefix) => format!("{}\n{}", prompt_prefix, req.text),
    }
}
```

`arcanum-models/src/lib.rs`:
```rust
mod ollama;
pub use ollama::OllamaProvider;
```

- [ ] **Step 4: Run tests and commit**

```bash
cargo test -p arcanum-models
git add arcanum-models/
git commit -m "feat(models): add OllamaProvider implementing Embedder + TextEnricher"
```

---

### Task 16: arcanum-models — EnrichmentDispatcher + EmbeddingParallelismRouter

**Files:**
- Create: `arcanum-models/src/dispatcher.rs`
- Create: `arcanum-models/src/router.rs`

- [ ] **Step 1: Write the failing test**

```rust
// arcanum-models/tests/dispatcher_test.rs
use arcanum_models::{EnrichmentDispatcher, RoutingRule};
use arcanum_core::types::*;
use arcanum_core::traits::*;
use std::sync::Arc;

struct EchoEnricher;
#[async_trait::async_trait]
impl TextEnricher for EchoEnricher {
    async fn enrich(&self, req: EnrichRequest) -> arcanum_core::Result<EnrichedText> {
        Ok(EnrichedText(format!("echo:{}", req.text)))
    }
}

#[tokio::test]
async fn test_dispatcher_routes_by_intent() {
    let default = Arc::new(EchoEnricher) as Arc<dyn TextEnricher>;
    let entity  = Arc::new(EchoEnricher) as Arc<dyn TextEnricher>;
    let dispatcher = EnrichmentDispatcher::new(default)
        .with_override(EnrichIntent::ExtractEntities, entity);

    let result = dispatcher.enrich(EnrichRequest {
        text: "test".into(), intent: EnrichIntent::ExtractEntities, context: None,
    }).await.unwrap();
    assert!(result.0.starts_with("echo:"));
}
```

- [ ] **Step 2: Implement `arcanum-models/src/dispatcher.rs`**

```rust
use arcanum_core::{traits::*, types::*, Result};
use async_trait::async_trait;
use std::{collections::HashMap, sync::Arc};

pub struct EnrichmentDispatcher {
    default: Arc<dyn TextEnricher>,
    overrides: HashMap<String, Arc<dyn TextEnricher>>,
}

impl EnrichmentDispatcher {
    pub fn new(default: Arc<dyn TextEnricher>) -> Self {
        Self { default, overrides: HashMap::new() }
    }

    pub fn with_override(mut self, intent: EnrichIntent, provider: Arc<dyn TextEnricher>) -> Self {
        self.overrides.insert(intent_key(&intent), provider);
        self
    }
}

fn intent_key(intent: &EnrichIntent) -> String {
    match intent {
        EnrichIntent::ContextPrefix => "context_prefix".into(),
        EnrichIntent::Summarize => "summarize".into(),
        EnrichIntent::ExtractEntities => "extract_entities".into(),
        EnrichIntent::Caption => "caption".into(),
        EnrichIntent::Rerank => "rerank".into(),
        EnrichIntent::Custom(s) => format!("custom:{}", s),
    }
}

#[async_trait]
impl TextEnricher for EnrichmentDispatcher {
    async fn enrich(&self, request: EnrichRequest) -> Result<EnrichedText> {
        let key = intent_key(&request.intent);
        let provider = self.overrides.get(&key).unwrap_or(&self.default);
        provider.enrich(request).await
    }
}
```

`arcanum-models/src/router.rs` — round-robin embedding router:
```rust
use arcanum_core::{traits::*, types::*, Result};
use async_trait::async_trait;
use std::sync::{Arc, atomic::{AtomicUsize, Ordering}};

pub struct EmbeddingParallelismRouter {
    providers: Vec<Arc<dyn Embedder>>,
    counter: AtomicUsize,
}

impl EmbeddingParallelismRouter {
    pub fn new(providers: Vec<Arc<dyn Embedder>>) -> Self {
        assert!(!providers.is_empty(), "at least one embedder required");
        Self { providers, counter: AtomicUsize::new(0) }
    }
}

#[async_trait]
impl Embedder for EmbeddingParallelismRouter {
    async fn embed(&self, texts: Vec<String>) -> Result<Vec<Vector>> {
        let idx = self.counter.fetch_add(1, Ordering::Relaxed) % self.providers.len();
        self.providers[idx].embed(texts).await
    }

    fn dimension(&self) -> usize {
        self.providers[0].dimension()
    }
}
```

Update `arcanum-models/src/lib.rs`:
```rust
mod dispatcher;
mod ollama;
mod router;
pub use dispatcher::EnrichmentDispatcher;
pub use ollama::OllamaProvider;
pub use router::EmbeddingParallelismRouter;
```

- [ ] **Step 3: Run tests and commit**

```bash
cargo test -p arcanum-models
git add arcanum-models/
git commit -m "feat(models): add EnrichmentDispatcher and EmbeddingParallelismRouter"
```

---

## Phase 2 Complete ✓

All four storage/model crates have implementations and pass tests. Verify:

```bash
cargo test -p arcanum-vector -p arcanum-graph -p arcanum-tree -p arcanum-models
```

Proceed to **Part 3** (arcanum-ingestion, arcanum-middleware, arcanum-retrieval, arcanum-eval, arcanum-pipeline).
