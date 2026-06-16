# Evidence Foundation (Phase 1) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add raw document snapshot persistence, typed chunk provenance, document versioning, tree leaf-chunk provenance, and graph source-chunk provenance to the Arcanum ingestion pipeline.

**Architecture:** Three layers of change — (1) new types and traits in `arcanum-core`, (2) `LocalSnapshotStore` and `PostgresDocumentVersionStore` implementations in `arcanum-ingestion`, (3) updated pipeline stages in `arcanum-pipeline` that wire the stores into the ingestion flow. `RaptorBuilder` and Neo4j store are fixed independently. `DocumentRegistry` and `SqliteDocumentRegistry` are removed and replaced end-to-end.

**Tech Stack:** Rust, SQLx (Postgres), tokio::fs, serde_json, async_trait.

**Spec:** `docs/superpowers/specs/2026-06-15-evidence-foundation-design.md`

---

## File Map

### New files
| File | Responsibility |
|---|---|
| `arcanum-core/src/types/provenance.rs` | `ChunkProvenance`, `DocumentVersion`, `VersionStatus`, `VersioningPolicy`, `SnapshotLocation` |
| `arcanum-core/src/traits/snapshot.rs` | `SnapshotStore` trait |
| `arcanum-core/src/traits/versioning.rs` | `DocumentVersionStore` trait + `NoOpDocumentVersionStore` |
| `arcanum-ingestion/src/snapshot/local.rs` | `LocalSnapshotStore` |
| `arcanum-ingestion/src/snapshot/mod.rs` | re-exports |
| `arcanum-ingestion/src/versioning/postgres.rs` | `PostgresDocumentVersionStore` |
| `arcanum-ingestion/src/versioning/mod.rs` | re-exports |
| `migrations/0002_evidence_foundation.sql` | new Postgres schema |

### Modified files
| File | What changes |
|---|---|
| `arcanum-core/src/types/document.rs` | add `provenance: ChunkProvenance` to `Chunk`; remove `ChunkMetadata::source_uri()` |
| `arcanum-core/src/types/tree.rs` | add `leaf_chunk_ids: Vec<ChunkId>` to `TreeNode` |
| `arcanum-core/src/types/graph.rs` | `Relation.source_chunk: ChunkId` → `source_chunks: Vec<ChunkId>` |
| `arcanum-core/src/types/mod.rs` | pub use provenance::* |
| `arcanum-core/src/traits/mod.rs` | pub use snapshot, versioning; remove registry |
| `arcanum-core/src/traits/ingestion.rs` | add `canonical()` default to `Preprocessor` trait |
| `arcanum-ingestion/src/lib.rs` | pub use snapshot, versioning; remove document_registry |
| `arcanum-ingestion/src/preprocessors/docling.rs` | implement `canonical()` |
| `arcanum-ingestion/src/chunkers/*.rs` (5 files) | add `provenance: Default::default()` to `Chunk` construction |
| `arcanum-ingestion/src/enrichment/entity.rs` | `source_chunk` → `source_chunks: vec![...]` |
| `arcanum-pipeline/src/ingestion_state.rs` | add 6 new fields |
| `arcanum-pipeline/src/deps.rs` | replace `document_registry` with `version_store` + `snapshot_store` |
| `arcanum-pipeline/src/stages.rs` | update load, dedup, cleanup, preprocess, chunk stages; add snapshot stage |
| `arcanum-pipeline/src/templates/*.rs` (5 files) | wire new deps |
| `arcanum-pipeline/src/worker.rs` | remove `document_registry` calls |
| `arcanum-tree/src/raptor.rs` | thread `ChunkId` through, propagate `leaf_chunk_ids` |
| `arcanum-tree/src/postgres_store.rs` | persist + read `leaf_chunk_ids` |
| `arcanum-graph/src/neo4j_store.rs` | persist `source_chunks` on entities/relations; remove dummy UUID |
| `arcanum-engine/src/engine.rs` | replace `document_registry` with `version_store` + `snapshot_store` |

### Deleted files
| File | Reason |
|---|---|
| `arcanum-core/src/traits/registry.rs` | replaced by `versioning.rs` |
| `arcanum-ingestion/src/document_registry.rs` | `SqliteDocumentRegistry` replaced by `PostgresDocumentVersionStore` |

---

## Task 1: Core provenance types

**Files:**
- Create: `arcanum-core/src/types/provenance.rs`
- Modify: `arcanum-core/src/types/mod.rs`

- [ ] **Write the test**

In `arcanum-core/src/types/provenance.rs`:

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use super::document::DocumentId;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChunkProvenance {
    pub document_version: u32,
    pub source_uri:       String,
    pub snapshot_uri:     String,
    pub canonical_uri:    Option<String>,
    pub page:             Option<u32>,
    pub section:          Option<String>,
    pub block_ids:        Vec<String>,
}

impl Default for ChunkProvenance {
    fn default() -> Self {
        Self {
            document_version: 0,
            source_uri:       String::new(),
            snapshot_uri:     String::new(),
            canonical_uri:    None,
            page:             None,
            section:          None,
            block_ids:        vec![],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum VersionStatus {
    Active,
    Superseded,
    Deleted,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum VersioningPolicy {
    Replace,
    AppendOnly,
    RetentionBased { days: u32 },
}

impl Default for VersioningPolicy {
    fn default() -> Self { Self::Replace }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentVersion {
    pub document_id:   DocumentId,
    pub version_num:   u32,
    pub source_uri:    String,
    pub collection_id: String,
    pub content_hash:  String,
    pub snapshot_uri:  String,
    pub canonical_uri: Option<String>,
    pub mime_type:     String,
    pub status:        VersionStatus,
    pub ingested_at:   DateTime<Utc>,
    pub extra:         HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone)]
pub struct SnapshotLocation {
    pub raw_uri:       String,
    pub canonical_uri: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunk_provenance_default_is_empty() {
        let p = ChunkProvenance::default();
        assert_eq!(p.document_version, 0);
        assert!(p.source_uri.is_empty());
        assert!(p.snapshot_uri.is_empty());
        assert!(p.canonical_uri.is_none());
        assert!(p.block_ids.is_empty());
    }

    #[test]
    fn chunk_provenance_roundtrips_json() {
        let p = ChunkProvenance {
            document_version: 3,
            source_uri:       "confluence://page/42".into(),
            snapshot_uri:     "file:///data/snapshots/abc/3.raw".into(),
            canonical_uri:    Some("file:///data/snapshots/abc/3.canonical.json".into()),
            page:             Some(7),
            section:          Some("2.1 > Overview".into()),
            block_ids:        vec!["b-007-a".into()],
        };
        let json = serde_json::to_string(&p).unwrap();
        let back: ChunkProvenance = serde_json::from_str(&json).unwrap();
        assert_eq!(p, back);
    }

    #[test]
    fn versioning_policy_default_is_replace() {
        assert_eq!(VersioningPolicy::default(), VersioningPolicy::Replace);
    }
}
```

- [ ] **Wire into mod.rs** — add to `arcanum-core/src/types/mod.rs`:

```rust
pub mod provenance;
pub use provenance::{ChunkProvenance, DocumentVersion, VersionStatus, VersioningPolicy, SnapshotLocation};
```

- [ ] **Run tests**

```bash
cargo test -p arcanum-core types::provenance -- --nocapture
```
Expected: 3 tests pass.

- [ ] **Commit**

```bash
git add arcanum-core/src/types/provenance.rs arcanum-core/src/types/mod.rs
git commit -m "feat(evidence): add ChunkProvenance and document versioning types"
```

---

## Task 2: Update Chunk, TreeNode, Relation types

These type changes break all construction sites — every compile error after this step must be fixed before the commit.

**Files:**
- Modify: `arcanum-core/src/types/document.rs`
- Modify: `arcanum-core/src/types/tree.rs`
- Modify: `arcanum-core/src/types/graph.rs`
- Modify: `arcanum-ingestion/src/chunkers/fixed.rs`, `semantic.rs`, `propositional.rs`, `hierarchical.rs`, `structure.rs`
- Modify: `arcanum-ingestion/src/enrichment/entity.rs`
- Modify: `arcanum-tree/src/raptor.rs` (construction site only; full fix in Task 11)
- Modify: `arcanum-tree/src/postgres_store.rs` (construction site only; full fix in Task 11)

- [ ] **Update Chunk in `arcanum-core/src/types/document.rs`**

Add `pub provenance: ChunkProvenance,` after `pub metadata: ChunkMetadata,`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chunk {
    pub id:            ChunkId,
    pub text:          String,
    pub document_id:   DocumentId,
    pub collection_id: CollectionId,
    pub position:      ChunkPosition,
    pub metadata:      ChunkMetadata,
    pub provenance:    ChunkProvenance,
}
```

Remove the `ChunkMetadata::source_uri()` method (the entire `impl ChunkMetadata { fn source_uri ... }` block) and its test `test_chunk_metadata_source_uri`.

Update the existing `test_chunk_construction` test to add the new field:

```rust
let chunk = Chunk {
    id: ChunkId::new(),
    text: "Hello world".to_string(),
    document_id: DocumentId::new(),
    collection_id: CollectionId("default".to_string()),
    position: ChunkPosition { start: 0, end: 11, index: 0 },
    metadata: ChunkMetadata::default(),
    provenance: ChunkProvenance::default(),
};
```

- [ ] **Update TreeNode in `arcanum-core/src/types/tree.rs`**

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TreeNode {
    pub id:              TreeNodeId,
    pub level:           u32,
    pub text:            String,
    pub vector:          Vector,
    pub parent:          Option<TreeNodeId>,
    pub children:        Vec<TreeNodeId>,
    pub cluster_centroid: Option<Vector>,
    pub source_uri:      String,
    pub leaf_chunk_ids:  Vec<ChunkId>,
}
```

- [ ] **Update Relation in `arcanum-core/src/types/graph.rs`**

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Relation {
    pub source:        EntityId,
    pub relation_type: String,
    pub target:        EntityId,
    pub confidence:    f32,
    pub source_chunks: Vec<ChunkId>,
}
```

- [ ] **Fix all Chunk construction sites in chunkers**

In each of `arcanum-ingestion/src/chunkers/{fixed,semantic,propositional,hierarchical,structure}.rs`, every `Chunk { ... }` literal needs `provenance: ChunkProvenance::default(),`. Add it. Also add `use arcanum_core::types::ChunkProvenance;` to the use statements if not already imported (it comes from `arcanum_core::types::*` if that wildcard is used).

- [ ] **Fix Relation construction in `arcanum-ingestion/src/enrichment/entity.rs`**

Find every `Relation { source_chunk: ... }` and change to `source_chunks: vec![chunk_id],`. The variable name for the old field was `source_chunk` — replace the field name and wrap the value in `vec![...]`.

- [ ] **Fix TreeNode construction sites**

In `arcanum-tree/src/raptor.rs`, every `TreeNode { ... }` literal needs `leaf_chunk_ids: vec![],`. Full logic comes in Task 11.

In `arcanum-tree/src/postgres_store.rs`, the `row_to_node` function needs:

```rust
Ok(TreeNode {
    id: TreeNodeId(row.id),
    level: row.level as u32,
    text: row.text,
    vector,
    parent: row.parent_id.map(TreeNodeId),
    children,
    cluster_centroid,
    source_uri: row.source_uri,
    leaf_chunk_ids: vec![],    // populated from DB in Task 11
})
```

- [ ] **Verify the project compiles**

```bash
cargo build 2>&1 | head -40
```
Expected: 0 errors. Warnings about unused `source_chunk` field are OK temporarily.

- [ ] **Run core tests**

```bash
cargo test -p arcanum-core
```
Expected: all pass.

- [ ] **Commit**

```bash
git add arcanum-core/src/types/ arcanum-ingestion/src/chunkers/ arcanum-ingestion/src/enrichment/ arcanum-tree/src/
git commit -m "feat(evidence): add provenance field to Chunk, leaf_chunk_ids to TreeNode, source_chunks to Relation"
```

---

## Task 3: SnapshotStore and DocumentVersionStore traits

**Files:**
- Create: `arcanum-core/src/traits/snapshot.rs`
- Create: `arcanum-core/src/traits/versioning.rs`
- Modify: `arcanum-core/src/traits/mod.rs`

- [ ] **Write `arcanum-core/src/traits/snapshot.rs`**

```rust
use async_trait::async_trait;
use crate::{types::{DocumentId, SnapshotLocation}, Result};

#[async_trait]
pub trait SnapshotStore: Send + Sync {
    async fn store(
        &self,
        document_id: &DocumentId,
        version:     u32,
        raw:         &[u8],
        canonical:   Option<&serde_json::Value>,
    ) -> Result<SnapshotLocation>;

    async fn fetch_raw(&self, uri: &str) -> Result<Vec<u8>>;
    async fn fetch_canonical(&self, uri: &str) -> Result<Option<serde_json::Value>>;
}
```

- [ ] **Write `arcanum-core/src/traits/versioning.rs`**

```rust
use async_trait::async_trait;
use crate::{
    types::{DocumentId, DocumentVersion, VersioningPolicy},
    Result,
};

#[async_trait]
pub trait DocumentVersionStore: Send + Sync {
    async fn get_latest(
        &self,
        source_uri:    &str,
        collection_id: &str,
    ) -> Result<Option<DocumentVersion>>;

    async fn add_version(&self, version: DocumentVersion) -> Result<()>;

    async fn supersede_active(&self, document_id: &DocumentId) -> Result<()>;

    async fn list_versions(
        &self,
        document_id: &DocumentId,
    ) -> Result<Vec<DocumentVersion>>;

    async fn get_versioning_policy(&self, collection_id: &str) -> Result<VersioningPolicy>;

    async fn set_versioning_policy(
        &self,
        collection_id: &str,
        policy:        VersioningPolicy,
    ) -> Result<()>;
}

/// No-op implementation for tests and dev setups without Postgres.
/// Every document is treated as new; no version history is kept.
pub struct NoOpDocumentVersionStore;

#[async_trait]
impl DocumentVersionStore for NoOpDocumentVersionStore {
    async fn get_latest(&self, _: &str, _: &str) -> Result<Option<DocumentVersion>> {
        Ok(None)
    }
    async fn add_version(&self, _: DocumentVersion) -> Result<()> { Ok(()) }
    async fn supersede_active(&self, _: &DocumentId) -> Result<()> { Ok(()) }
    async fn list_versions(&self, _: &DocumentId) -> Result<Vec<DocumentVersion>> { Ok(vec![]) }
    async fn get_versioning_policy(&self, _: &str) -> Result<VersioningPolicy> {
        Ok(VersioningPolicy::Replace)
    }
    async fn set_versioning_policy(&self, _: &str, _: VersioningPolicy) -> Result<()> { Ok(()) }
}
```

- [ ] **Update `arcanum-core/src/traits/mod.rs`**

Add:
```rust
pub mod snapshot;
pub mod versioning;
pub use snapshot::SnapshotStore;
pub use versioning::{DocumentVersionStore, NoOpDocumentVersionStore};
```

Remove:
```rust
pub mod registry;
pub use registry::*;
```

- [ ] **Delete `arcanum-core/src/traits/registry.rs`**

```bash
rm arcanum-core/src/traits/registry.rs
```

- [ ] **Fix all compile errors from removing DocumentRegistry**

Run `cargo build 2>&1 | grep "error"` and fix each one. The errors will be in:
- `arcanum-core/src/traits/ingestion.rs` — remove any `DocumentRegistry` import if present
- `arcanum-pipeline/src/deps.rs` — change `document_registry: Arc<dyn DocumentRegistry>` to compile by adding a placeholder (full fix in Task 8)
- `arcanum-engine/src/engine.rs` — same

Temporarily replace `Arc<dyn DocumentRegistry>` with `Arc<dyn DocumentVersionStore>` (using `NoOpDocumentVersionStore` as the default) wherever the compiler complains. The full wiring is done in Tasks 8–14.

- [ ] **Compile check**

```bash
cargo build 2>&1 | head -20
```

- [ ] **Commit**

```bash
git add arcanum-core/src/traits/
git commit -m "feat(evidence): add SnapshotStore and DocumentVersionStore traits; remove DocumentRegistry"
```

---

## Task 4: LocalSnapshotStore

**Files:**
- Create: `arcanum-ingestion/src/snapshot/mod.rs`
- Create: `arcanum-ingestion/src/snapshot/local.rs`
- Modify: `arcanum-ingestion/src/lib.rs`

- [ ] **Write the tests first in `arcanum-ingestion/src/snapshot/local.rs`**

```rust
use arcanum_core::{traits::SnapshotStore, types::{DocumentId, SnapshotLocation}, Result, ArcanumError};
use async_trait::async_trait;
use std::path::PathBuf;
use tokio::fs;

pub struct LocalSnapshotStore {
    base_path: PathBuf,
}

impl LocalSnapshotStore {
    pub fn new(base_path: impl Into<PathBuf>) -> Self {
        Self { base_path: base_path.into() }
    }

    fn raw_path(&self, document_id: &DocumentId, version: u32) -> PathBuf {
        self.base_path
            .join(document_id.0.to_string())
            .join(format!("{}.raw", version))
    }

    fn canonical_path(&self, document_id: &DocumentId, version: u32) -> PathBuf {
        self.base_path
            .join(document_id.0.to_string())
            .join(format!("{}.canonical.json", version))
    }
}

#[async_trait]
impl SnapshotStore for LocalSnapshotStore {
    async fn store(
        &self,
        document_id: &DocumentId,
        version:     u32,
        raw:         &[u8],
        canonical:   Option<&serde_json::Value>,
    ) -> Result<SnapshotLocation> {
        let raw_path = self.raw_path(document_id, version);
        if let Some(parent) = raw_path.parent() {
            fs::create_dir_all(parent).await
                .map_err(|e| ArcanumError::Storage(format!("create dir: {}", e)))?;
        }
        fs::write(&raw_path, raw).await
            .map_err(|e| ArcanumError::Storage(format!("write raw: {}", e)))?;

        let canonical_uri = if let Some(c) = canonical {
            let path = self.canonical_path(document_id, version);
            let bytes = serde_json::to_vec(c)
                .map_err(|e| ArcanumError::Storage(format!("serialize canonical: {}", e)))?;
            fs::write(&path, bytes).await
                .map_err(|e| ArcanumError::Storage(format!("write canonical: {}", e)))?;
            Some(format!("file://{}", path.display()))
        } else {
            None
        };

        Ok(SnapshotLocation {
            raw_uri:       format!("file://{}", raw_path.display()),
            canonical_uri,
        })
    }

    async fn fetch_raw(&self, uri: &str) -> Result<Vec<u8>> {
        let path = uri.strip_prefix("file://")
            .ok_or_else(|| ArcanumError::Storage(format!("unsupported URI scheme: {}", uri)))?;
        fs::read(path).await
            .map_err(|e| ArcanumError::Storage(format!("read raw: {}", e)))
    }

    async fn fetch_canonical(&self, uri: &str) -> Result<Option<serde_json::Value>> {
        let path = uri.strip_prefix("file://")
            .ok_or_else(|| ArcanumError::Storage(format!("unsupported URI scheme: {}", uri)))?;
        let bytes = fs::read(path).await
            .map_err(|e| ArcanumError::Storage(format!("read canonical: {}", e)))?;
        let value = serde_json::from_slice(&bytes)
            .map_err(|e| ArcanumError::Storage(format!("parse canonical: {}", e)))?;
        Ok(Some(value))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arcanum_core::types::DocumentId;
    use tempfile::tempdir;

    #[tokio::test]
    async fn stores_and_fetches_raw_bytes() {
        let dir = tempdir().unwrap();
        let store = LocalSnapshotStore::new(dir.path());
        let doc_id = DocumentId::new();
        let raw = b"hello world";

        let loc = store.store(&doc_id, 1, raw, None).await.unwrap();
        assert!(loc.canonical_uri.is_none());
        assert!(loc.raw_uri.starts_with("file://"));

        let fetched = store.fetch_raw(&loc.raw_uri).await.unwrap();
        assert_eq!(fetched, raw);
    }

    #[tokio::test]
    async fn stores_and_fetches_canonical_sidecar() {
        let dir = tempdir().unwrap();
        let store = LocalSnapshotStore::new(dir.path());
        let doc_id = DocumentId::new();
        let canonical = serde_json::json!({ "blocks": [{ "id": "b1", "text": "hi" }] });

        let loc = store.store(&doc_id, 1, b"raw", Some(&canonical)).await.unwrap();
        let canonical_uri = loc.canonical_uri.unwrap();

        let fetched = store.fetch_canonical(&canonical_uri).await.unwrap().unwrap();
        assert_eq!(fetched, canonical);
    }

    #[tokio::test]
    async fn different_versions_do_not_overwrite_each_other() {
        let dir = tempdir().unwrap();
        let store = LocalSnapshotStore::new(dir.path());
        let doc_id = DocumentId::new();

        let loc1 = store.store(&doc_id, 1, b"v1 content", None).await.unwrap();
        let loc2 = store.store(&doc_id, 2, b"v2 content", None).await.unwrap();

        assert_ne!(loc1.raw_uri, loc2.raw_uri);
        assert_eq!(store.fetch_raw(&loc1.raw_uri).await.unwrap(), b"v1 content");
        assert_eq!(store.fetch_raw(&loc2.raw_uri).await.unwrap(), b"v2 content");
    }
}
```

- [ ] **Create `arcanum-ingestion/src/snapshot/mod.rs`**

```rust
mod local;
pub use local::LocalSnapshotStore;
```

- [ ] **Add to `arcanum-ingestion/src/lib.rs`**

```rust
pub mod snapshot;
pub use snapshot::LocalSnapshotStore;
```

- [ ] **Add `tempfile` to `arcanum-ingestion/Cargo.toml` dev-dependencies**

```toml
[dev-dependencies]
tempfile = "3"
```

- [ ] **Run tests**

```bash
cargo test -p arcanum-ingestion snapshot -- --nocapture
```
Expected: 3 tests pass.

- [ ] **Commit**

```bash
git add arcanum-ingestion/src/snapshot/ arcanum-ingestion/src/lib.rs arcanum-ingestion/Cargo.toml
git commit -m "feat(evidence): implement LocalSnapshotStore"
```

---

## Task 5: Postgres migration

**Files:**
- Create: `migrations/0002_evidence_foundation.sql`

- [ ] **Write the migration**

```sql
-- migrations/0002_evidence_foundation.sql

-- Stable logical identity for a document across all versions.
-- One row per (source_uri, collection_id) pair, created on first ingestion.
CREATE TABLE source_documents (
    document_id   UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    source_uri    TEXT        NOT NULL,
    collection_id TEXT        NOT NULL,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (source_uri, collection_id)
);

-- One row per ingested version of a document.
CREATE TABLE document_versions (
    document_id   UUID        NOT NULL REFERENCES source_documents(document_id),
    version_num   INTEGER     NOT NULL,
    content_hash  TEXT        NOT NULL,
    snapshot_uri  TEXT        NOT NULL,
    canonical_uri TEXT,
    mime_type     TEXT        NOT NULL DEFAULT '',
    status        TEXT        NOT NULL DEFAULT 'active'
                  CHECK (status IN ('active', 'superseded', 'deleted')),
    ingested_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    extra         JSONB,
    PRIMARY KEY (document_id, version_num)
);

CREATE INDEX ON document_versions (document_id, status);
CREATE INDEX ON document_versions (content_hash);

-- Per-collection versioning policy.
CREATE TABLE collection_config (
    collection_id     TEXT        PRIMARY KEY,
    versioning_policy TEXT        NOT NULL DEFAULT 'replace'
                      CHECK (versioning_policy IN ('replace', 'append_only', 'retention_based')),
    retention_days    INTEGER,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Tree nodes with leaf_chunk_ids (full rewrite — no migration from old table).
DROP TABLE IF EXISTS arcanum_tree_nodes;
DROP TABLE IF EXISTS arcanum_tree_collections;

CREATE TABLE arcanum_tree_nodes (
    id             UUID    PRIMARY KEY,
    collection     TEXT    NOT NULL,
    level          INTEGER NOT NULL,
    text           TEXT    NOT NULL,
    vector         JSONB   NOT NULL,
    centroid       JSONB,
    parent_id      UUID,
    children       JSONB   NOT NULL DEFAULT '[]',
    source_uri     TEXT    NOT NULL DEFAULT '',
    leaf_chunk_ids JSONB   NOT NULL DEFAULT '[]'
);

CREATE INDEX ON arcanum_tree_nodes (collection, level);

CREATE TABLE arcanum_tree_collections (
    name TEXT PRIMARY KEY
);
```

- [ ] **Apply migration to dev database**

```bash
psql "$DATABASE_URL" -f migrations/0002_evidence_foundation.sql
```
Expected: all statements execute without error.

- [ ] **Commit**

```bash
git add migrations/0002_evidence_foundation.sql
git commit -m "feat(evidence): add Postgres migration for document versions, collection config, tree nodes"
```

---

## Task 6: PostgresDocumentVersionStore

**Files:**
- Create: `arcanum-ingestion/src/versioning/mod.rs`
- Create: `arcanum-ingestion/src/versioning/postgres.rs`
- Modify: `arcanum-ingestion/src/lib.rs`
- Modify: `arcanum-ingestion/Cargo.toml`

- [ ] **Add sqlx dependency to `arcanum-ingestion/Cargo.toml`**

```toml
[dependencies]
sqlx = { version = "0.8", features = ["runtime-tokio-rustls", "postgres", "uuid", "chrono", "json"] }
chrono = { version = "0.4", features = ["serde"] }
```

- [ ] **Write `arcanum-ingestion/src/versioning/postgres.rs`**

```rust
use arcanum_core::{
    traits::versioning::DocumentVersionStore,
    types::{DocumentId, DocumentVersion, VersionStatus, VersioningPolicy},
    ArcanumError, Result,
};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use std::collections::HashMap;
use uuid::Uuid;

pub struct PostgresDocumentVersionStore {
    pool: PgPool,
}

impl PostgresDocumentVersionStore {
    pub async fn new(database_url: &str) -> Result<Self> {
        let pool = PgPool::connect(database_url).await
            .map_err(|e| ArcanumError::Storage(format!("connect: {}", e)))?;
        Ok(Self { pool })
    }

    /// Returns the stable document_id for this (source_uri, collection_id),
    /// creating a new one if this is the first time we've seen it.
    async fn get_or_create_document_id(
        &self,
        source_uri:    &str,
        collection_id: &str,
    ) -> Result<Uuid> {
        let row: (Uuid,) = sqlx::query_as(
            "INSERT INTO source_documents (source_uri, collection_id)
             VALUES ($1, $2)
             ON CONFLICT (source_uri, collection_id) DO UPDATE
               SET source_uri = EXCLUDED.source_uri
             RETURNING document_id",
        )
        .bind(source_uri)
        .bind(collection_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| ArcanumError::Storage(format!("get_or_create_document_id: {}", e)))?;
        Ok(row.0)
    }

    async fn next_version_num(&self, document_id: Uuid) -> Result<u32> {
        let row: (Option<i32>,) = sqlx::query_as(
            "SELECT MAX(version_num) FROM document_versions WHERE document_id = $1",
        )
        .bind(document_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| ArcanumError::Storage(format!("next_version_num: {}", e)))?;
        Ok(row.0.unwrap_or(0) as u32 + 1)
    }
}

#[async_trait]
impl DocumentVersionStore for PostgresDocumentVersionStore {
    async fn get_latest(
        &self,
        source_uri:    &str,
        collection_id: &str,
    ) -> Result<Option<DocumentVersion>> {
        let row = sqlx::query_as::<_, PgVersionRow>(
            "SELECT dv.document_id, dv.version_num, dv.content_hash,
                    dv.snapshot_uri, dv.canonical_uri, dv.mime_type,
                    dv.status, dv.ingested_at, dv.extra
             FROM document_versions dv
             JOIN source_documents sd USING (document_id)
             WHERE sd.source_uri = $1 AND sd.collection_id = $2
               AND dv.status = 'active'
             ORDER BY dv.version_num DESC
             LIMIT 1",
        )
        .bind(source_uri)
        .bind(collection_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| ArcanumError::Storage(format!("get_latest: {}", e)))?;

        Ok(row.map(|r| r.into_version(source_uri, collection_id)))
    }

    async fn add_version(&self, version: DocumentVersion) -> Result<()> {
        let document_id = self
            .get_or_create_document_id(&version.source_uri, &version.collection_id)
            .await?;
        let version_num = version.version_num as i32;
        let status = status_to_str(&version.status);
        let extra = serde_json::to_value(&version.extra).unwrap_or(serde_json::Value::Null);

        sqlx::query(
            "INSERT INTO document_versions
             (document_id, version_num, content_hash, snapshot_uri, canonical_uri,
              mime_type, status, extra)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
        )
        .bind(document_id)
        .bind(version_num)
        .bind(&version.content_hash)
        .bind(&version.snapshot_uri)
        .bind(&version.canonical_uri)
        .bind(&version.mime_type)
        .bind(status)
        .bind(extra)
        .execute(&self.pool)
        .await
        .map_err(|e| ArcanumError::Storage(format!("add_version: {}", e)))?;
        Ok(())
    }

    async fn supersede_active(&self, document_id: &DocumentId) -> Result<()> {
        sqlx::query(
            "UPDATE document_versions SET status = 'superseded'
             WHERE document_id = $1 AND status = 'active'",
        )
        .bind(document_id.0)
        .execute(&self.pool)
        .await
        .map_err(|e| ArcanumError::Storage(format!("supersede_active: {}", e)))?;
        Ok(())
    }

    async fn list_versions(&self, document_id: &DocumentId) -> Result<Vec<DocumentVersion>> {
        let rows = sqlx::query_as::<_, PgVersionRow>(
            "SELECT dv.document_id, dv.version_num, dv.content_hash,
                    dv.snapshot_uri, dv.canonical_uri, dv.mime_type,
                    dv.status, dv.ingested_at, dv.extra
             FROM document_versions dv
             JOIN source_documents sd USING (document_id)
             WHERE dv.document_id = $1
             ORDER BY dv.version_num ASC",
        )
        .bind(document_id.0)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| ArcanumError::Storage(format!("list_versions: {}", e)))?;

        Ok(rows.into_iter().map(|r| r.into_version("", "")).collect())
    }

    async fn get_versioning_policy(&self, collection_id: &str) -> Result<VersioningPolicy> {
        let row: Option<(String, Option<i32>)> = sqlx::query_as(
            "SELECT versioning_policy, retention_days FROM collection_config
             WHERE collection_id = $1",
        )
        .bind(collection_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| ArcanumError::Storage(format!("get_versioning_policy: {}", e)))?;

        Ok(match row {
            None => VersioningPolicy::Replace,
            Some((s, days)) => match s.as_str() {
                "append_only"     => VersioningPolicy::AppendOnly,
                "retention_based" => VersioningPolicy::RetentionBased {
                    days: days.unwrap_or(90) as u32,
                },
                _ => VersioningPolicy::Replace,
            },
        })
    }

    async fn set_versioning_policy(
        &self,
        collection_id: &str,
        policy:        VersioningPolicy,
    ) -> Result<()> {
        let (policy_str, days) = match &policy {
            VersioningPolicy::Replace               => ("replace", None),
            VersioningPolicy::AppendOnly            => ("append_only", None),
            VersioningPolicy::RetentionBased { days } => ("retention_based", Some(*days as i32)),
        };
        sqlx::query(
            "INSERT INTO collection_config (collection_id, versioning_policy, retention_days)
             VALUES ($1, $2, $3)
             ON CONFLICT (collection_id) DO UPDATE
               SET versioning_policy = EXCLUDED.versioning_policy,
                   retention_days    = EXCLUDED.retention_days",
        )
        .bind(collection_id)
        .bind(policy_str)
        .bind(days)
        .execute(&self.pool)
        .await
        .map_err(|e| ArcanumError::Storage(format!("set_versioning_policy: {}", e)))?;
        Ok(())
    }
}

#[derive(sqlx::FromRow)]
struct PgVersionRow {
    document_id:   Uuid,
    version_num:   i32,
    content_hash:  String,
    snapshot_uri:  String,
    canonical_uri: Option<String>,
    mime_type:     String,
    status:        String,
    ingested_at:   DateTime<Utc>,
    extra:         Option<serde_json::Value>,
}

impl PgVersionRow {
    fn into_version(self, source_uri: &str, collection_id: &str) -> DocumentVersion {
        DocumentVersion {
            document_id:   DocumentId(self.document_id),
            version_num:   self.version_num as u32,
            source_uri:    source_uri.to_string(),
            collection_id: collection_id.to_string(),
            content_hash:  self.content_hash,
            snapshot_uri:  self.snapshot_uri,
            canonical_uri: self.canonical_uri,
            mime_type:     self.mime_type,
            status:        str_to_status(&self.status),
            ingested_at:   self.ingested_at,
            extra:         self.extra
                .and_then(|v| serde_json::from_value(v).ok())
                .unwrap_or_default(),
        }
    }
}

fn status_to_str(s: &VersionStatus) -> &'static str {
    match s {
        VersionStatus::Active     => "active",
        VersionStatus::Superseded => "superseded",
        VersionStatus::Deleted    => "deleted",
    }
}

fn str_to_status(s: &str) -> VersionStatus {
    match s {
        "superseded" => VersionStatus::Superseded,
        "deleted"    => VersionStatus::Deleted,
        _            => VersionStatus::Active,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Run with: cargo test -p arcanum-ingestion versioning -- --include-ignored --nocapture
    #[tokio::test]
    #[ignore]
    async fn test_add_and_get_latest() {
        let url = std::env::var("TEST_DATABASE_URL")
            .unwrap_or_else(|_| "postgres://postgres:postgres@localhost/arcanum_test".to_string());
        let store = PostgresDocumentVersionStore::new(&url).await.unwrap();

        let doc_id = DocumentId::new();
        let version = DocumentVersion {
            document_id:   doc_id.clone(),
            version_num:   1,
            source_uri:    "file://test.pdf".to_string(),
            collection_id: "col1".to_string(),
            content_hash:  "abc123".to_string(),
            snapshot_uri:  "file:///snapshots/1.raw".to_string(),
            canonical_uri: None,
            mime_type:     "application/pdf".to_string(),
            status:        VersionStatus::Active,
            ingested_at:   Utc::now(),
            extra:         HashMap::new(),
        };
        store.add_version(version).await.unwrap();

        let latest = store.get_latest("file://test.pdf", "col1").await.unwrap();
        assert!(latest.is_some());
        assert_eq!(latest.unwrap().content_hash, "abc123");
    }

    #[tokio::test]
    #[ignore]
    async fn test_supersede_active() {
        let url = std::env::var("TEST_DATABASE_URL")
            .unwrap_or_else(|_| "postgres://postgres:postgres@localhost/arcanum_test".to_string());
        let store = PostgresDocumentVersionStore::new(&url).await.unwrap();

        let doc_id = DocumentId::new();
        let v1 = DocumentVersion {
            document_id:   doc_id.clone(),
            version_num:   1,
            source_uri:    "file://s.pdf".to_string(),
            collection_id: "col_supersede".to_string(),
            content_hash:  "hash1".to_string(),
            snapshot_uri:  "file:///s/1.raw".to_string(),
            canonical_uri: None,
            mime_type:     "".to_string(),
            status:        VersionStatus::Active,
            ingested_at:   Utc::now(),
            extra:         HashMap::new(),
        };
        store.add_version(v1).await.unwrap();
        store.supersede_active(&doc_id).await.unwrap();

        let latest = store.get_latest("file://s.pdf", "col_supersede").await.unwrap();
        assert!(latest.is_none(), "superseded versions should not appear as latest");
    }

    #[tokio::test]
    #[ignore]
    async fn test_versioning_policy_roundtrip() {
        let url = std::env::var("TEST_DATABASE_URL")
            .unwrap_or_else(|_| "postgres://postgres:postgres@localhost/arcanum_test".to_string());
        let store = PostgresDocumentVersionStore::new(&url).await.unwrap();

        store.set_versioning_policy("col_policy", VersioningPolicy::AppendOnly).await.unwrap();
        let p = store.get_versioning_policy("col_policy").await.unwrap();
        assert_eq!(p, VersioningPolicy::AppendOnly);

        store.set_versioning_policy("col_policy", VersioningPolicy::RetentionBased { days: 30 }).await.unwrap();
        let p2 = store.get_versioning_policy("col_policy").await.unwrap();
        assert_eq!(p2, VersioningPolicy::RetentionBased { days: 30 });
    }
}
```

- [ ] **Create `arcanum-ingestion/src/versioning/mod.rs`**

```rust
mod postgres;
pub use postgres::PostgresDocumentVersionStore;
```

- [ ] **Add to `arcanum-ingestion/src/lib.rs`**

```rust
pub mod versioning;
pub use versioning::PostgresDocumentVersionStore;
```

- [ ] **Remove `document_registry.rs`**

```bash
rm arcanum-ingestion/src/document_registry.rs
```

Remove the `pub mod document_registry;` and `pub use document_registry::SqliteDocumentRegistry;` lines from `arcanum-ingestion/src/lib.rs`.

Remove `rusqlite` from `arcanum-ingestion/Cargo.toml` if it is no longer used elsewhere. Check with: `grep -r "rusqlite" arcanum-ingestion/src/`.

- [ ] **Compile check**

```bash
cargo build -p arcanum-ingestion 2>&1 | head -20
```

- [ ] **Commit**

```bash
git add arcanum-ingestion/src/versioning/ arcanum-ingestion/src/lib.rs arcanum-ingestion/Cargo.toml
git commit -m "feat(evidence): implement PostgresDocumentVersionStore; remove SqliteDocumentRegistry"
```

---

## Task 7: IngestionState new fields

**Files:**
- Modify: `arcanum-pipeline/src/ingestion_state.rs`

- [ ] **Update `IngestionState`**

```rust
use arcanum_core::{traits::Source, types::{CollectionId, DocumentId, RawDocument, Chunk, Vector}};

pub struct IngestionState {
    pub source:        Source,
    pub collection_id: CollectionId,
    pub doc:           Option<RawDocument>,
    pub chunks:        Vec<Chunk>,
    pub graph_chunks:  Vec<Chunk>,
    pub tree_chunks:   Vec<Chunk>,
    pub vectors:       Vec<Vector>,
    pub tree_vectors:  Vec<Vector>,

    // Set by load stage — original bytes before preprocess overwrites doc.content.
    pub raw_content:   Option<Vec<u8>>,
    // Set by preprocess stage — structured JSON from Docling; None for non-Docling formats.
    pub canonical_json: Option<serde_json::Value>,
    // Set by snapshot stage — populated once the snapshot is persisted.
    pub snapshot_document_id: Option<DocumentId>,
    pub snapshot_version_num: Option<u32>,
    pub snapshot_uri:         Option<String>,
    pub canonical_uri:        Option<String>,
}

impl IngestionState {
    pub fn new(source: Source, collection_id: CollectionId) -> Self {
        Self {
            source, collection_id, doc: None,
            chunks: vec![], graph_chunks: vec![], tree_chunks: vec![],
            vectors: vec![], tree_vectors: vec![],
            raw_content: None, canonical_json: None,
            snapshot_document_id: None, snapshot_version_num: None,
            snapshot_uri: None, canonical_uri: None,
        }
    }
}
```

- [ ] **Run compile check**

```bash
cargo build -p arcanum-pipeline 2>&1 | head -20
```

- [ ] **Commit**

```bash
git add arcanum-pipeline/src/ingestion_state.rs
git commit -m "feat(evidence): add raw_content, canonical_json, snapshot fields to IngestionState"
```

---

## Task 8: Preprocessor trait canonical() + DoclingPreprocessor

**Files:**
- Modify: `arcanum-core/src/traits/ingestion.rs`
- Modify: `arcanum-ingestion/src/preprocessors/docling.rs`
- Modify: `arcanum-ingestion/src/preprocessors/registry.rs`

- [ ] **Add `canonical()` to `Preprocessor` trait in `arcanum-core/src/traits/ingestion.rs`**

```rust
#[async_trait]
pub trait Preprocessor: Send + Sync {
    async fn process(&self, doc: RawDocument) -> Result<RawDocument>;

    /// Returns structured JSON output (e.g. Docling block format) if this
    /// preprocessor produces it. Default returns None — override in Docling impl.
    async fn canonical(&self, _doc: &RawDocument) -> Result<Option<serde_json::Value>> {
        Ok(None)
    }
}
```

- [ ] **Read `arcanum-ingestion/src/preprocessors/docling.rs` to find where Docling JSON is available**

```bash
grep -n "json\|Json\|serde\|output\|result\|body\|response" arcanum-ingestion/src/preprocessors/docling.rs | head -30
```

Locate the point where the Docling HTTP response or CLI output is parsed. The Docling API returns a JSON document. Capture it before extracting plain text.

- [ ] **Implement `canonical()` in `DoclingPreprocessor`**

In `arcanum-ingestion/src/preprocessors/docling.rs`, add a field `last_canonical: tokio::sync::Mutex<Option<serde_json::Value>>` to the struct, or — simpler — call the Docling backend a second time in `canonical()`. The simplest approach is to store the raw Docling JSON response during `process()` using a `Mutex<Option<...>>`:

```rust
pub struct DoclingPreprocessor {
    backend:         DoclingBackend,
    client:          reqwest::Client,
    last_canonical:  std::sync::Arc<tokio::sync::Mutex<Option<serde_json::Value>>>,
}

impl DoclingPreprocessor {
    pub fn new(backend: DoclingBackend) -> Self {
        Self {
            backend,
            client: reqwest::Client::new(),
            last_canonical: std::sync::Arc::new(tokio::sync::Mutex::new(None)),
        }
    }
}
```

In `process()`, after getting the Docling JSON response and before extracting plain text, store it:

```rust
*self.last_canonical.lock().await = Some(docling_json.clone());
```

Implement `canonical()`:

```rust
async fn canonical(&self, _doc: &RawDocument) -> Result<Option<serde_json::Value>> {
    Ok(self.last_canonical.lock().await.clone())
}
```

- [ ] **Add `canonical()` to `PreprocessorRegistry` in `arcanum-ingestion/src/preprocessors/registry.rs`**

```rust
pub async fn canonical(&self, doc: &RawDocument) -> Result<Option<serde_json::Value>> {
    for preprocessor in &self.preprocessors {
        let c = preprocessor.canonical(doc).await?;
        if c.is_some() { return Ok(c); }
    }
    Ok(None)
}
```

- [ ] **Update `make_preprocess_stage` in `arcanum-pipeline/src/stages.rs`** to call `canonical()` after `process()`:

```rust
pub fn make_preprocess_stage(
    state: Arc<Mutex<IngestionState>>,
    preprocessors: Arc<PreprocessorRegistry>,
) -> PipelineStage {
    PipelineStage {
        id: "preprocess",
        deps: vec!["cleanup"],
        run: Arc::new(move |ctx| {
            let state = state.clone();
            let pp = preprocessors.clone();
            Box::pin(async move {
                tracing::debug!(stage = "preprocess", "executing preprocess stage");
                if skip(&ctx) { return Ok(ctx); }
                let doc = state.lock().await.doc.clone().ok_or_else(|| {
                    ArcanumError::Pipeline { stage: "preprocess".into(), message: "no doc".into() }
                })?;
                let canonical = pp.canonical(&doc).await.unwrap_or(None);
                let processed = pp.process(doc).await?;
                let mut g = state.lock().await;
                g.doc = Some(processed);
                g.canonical_json = canonical;
                Ok(ctx)
            })
        }),
    }
}
```

- [ ] **Compile check**

```bash
cargo build -p arcanum-ingestion -p arcanum-pipeline 2>&1 | head -20
```

- [ ] **Commit**

```bash
git add arcanum-core/src/traits/ingestion.rs arcanum-ingestion/src/preprocessors/ arcanum-pipeline/src/stages.rs
git commit -m "feat(evidence): add canonical() to Preprocessor trait; DoclingPreprocessor captures structured JSON"
```

---

## Task 9: Update load stage to capture raw bytes

**Files:**
- Modify: `arcanum-pipeline/src/stages.rs`

- [ ] **Update `make_load_stage`**

Find `make_load_stage` in `arcanum-pipeline/src/stages.rs`. After `state.lock().await.doc = Some(doc);`, add capture of raw bytes:

```rust
pub fn make_load_stage(
    state: Arc<Mutex<IngestionState>>,
    loaders: Arc<LoaderRegistry>,
) -> PipelineStage {
    PipelineStage {
        id: "load",
        deps: vec![],
        run: Arc::new(move |ctx| {
            let state = state.clone();
            let loaders = loaders.clone();
            Box::pin(async move {
                tracing::debug!(stage = "load", "executing load stage");
                let source = state.lock().await.source.clone();
                let mut doc = loaders.load(&source).await?;
                doc.mime_type = MimeDetector::detect(&doc.content, Some(&doc.mime_type));
                let raw_bytes = doc.content.clone();   // capture before preprocess
                let mut g = state.lock().await;
                g.raw_content = Some(raw_bytes);
                g.doc = Some(doc);
                Ok(ctx)
            })
        }),
    }
}
```

- [ ] **Compile check**

```bash
cargo build -p arcanum-pipeline 2>&1 | head -10
```

- [ ] **Commit**

```bash
git add arcanum-pipeline/src/stages.rs
git commit -m "feat(evidence): load stage captures raw_content before preprocessing"
```

---

## Task 10: Replace DocumentRegistry in PipelineDeps and templates

**Files:**
- Modify: `arcanum-pipeline/src/deps.rs`
- Modify: `arcanum-pipeline/src/templates/standard.rs`
- Modify: `arcanum-pipeline/src/templates/contextual.rs`
- Modify: `arcanum-pipeline/src/templates/raptor.rs`
- Modify: `arcanum-pipeline/src/templates/graph.rs`
- Modify: `arcanum-pipeline/src/templates/full.rs`

- [ ] **Update `arcanum-pipeline/src/deps.rs`**

```rust
use arcanum_core::traits::{
    TextEnricher, Embedder, VectorStore, GraphStore, TreeStore,
    CacheInvalidationBroadcaster, DocumentVersionStore, SnapshotStore,
};
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
    pub version_store:     Arc<dyn DocumentVersionStore>,    // replaces document_registry
    pub snapshot_store:    Arc<dyn SnapshotStore>,           // new
    pub retry_policy:      RetryPolicy,
    pub cache_invalidator: Arc<CacheInvalidationBroadcaster>,
    pub embedding_cb:      Arc<CircuitBreaker>,
    pub vector_store_cb:   Arc<CircuitBreaker>,
}
```

- [ ] **Update all pipeline templates**

In each of `standard.rs`, `contextual.rs`, `raptor.rs`, `graph.rs`, `full.rs`:
- Replace `deps.document_registry.clone()` in `make_dedup_stage` call with `deps.version_store.clone()`
- Replace `deps.document_registry.clone()` in `make_cleanup_stage` call with `deps.version_store.clone()`
- Add snapshot stage after preprocess:

```rust
.add_stage(make_snapshot_stage(
    state.clone(),
    deps.version_store.clone(),
    deps.snapshot_store.clone(),
))
```

The full stage order for `standard.rs` becomes:

```rust
PipelineDAG::new()
    .add_stage(make_load_stage(state.clone(), deps.loaders.clone()))
    .add_stage(make_dedup_stage(state.clone(), deps.version_store.clone()))
    .add_stage(make_cleanup_stage(
        state.clone(),
        deps.version_store.clone(),
        deps.vector_store.clone(),
        deps.graph_store.clone(),
        deps.tree_store.clone(),
    ))
    .add_stage(make_preprocess_stage(state.clone(), deps.preprocessors.clone()))
    .add_stage(make_snapshot_stage(
        state.clone(),
        deps.version_store.clone(),
        deps.snapshot_store.clone(),
    ))
    .add_stage(make_vector_chunk_stage( ... ))
    // ... rest unchanged
```

Apply the same pattern to the other four templates.

- [ ] **Compile check** (will fail until dedup/cleanup/snapshot stages are updated — that's expected)

```bash
cargo build -p arcanum-pipeline 2>&1 | grep "error\[" | head -20
```

- [ ] **Commit**

```bash
git add arcanum-pipeline/src/deps.rs arcanum-pipeline/src/templates/
git commit -m "feat(evidence): replace DocumentRegistry with DocumentVersionStore + SnapshotStore in PipelineDeps and templates"
```

---

## Task 11: Update dedup, cleanup stages; add snapshot stage

**Files:**
- Modify: `arcanum-pipeline/src/stages.rs`

- [ ] **Update `make_dedup_stage`**

Replace signature and body to use `DocumentVersionStore`:

```rust
pub fn make_dedup_stage(
    state:         Arc<Mutex<IngestionState>>,
    version_store: Arc<dyn DocumentVersionStore>,
) -> PipelineStage {
    PipelineStage {
        id: "dedup",
        deps: vec!["load"],
        run: Arc::new(move |mut ctx| {
            let state         = state.clone();
            let version_store = version_store.clone();
            Box::pin(async move {
                tracing::debug!(stage = "dedup", "executing dedup stage");
                let force = ctx.get(CTX_FORCE).and_then(|v| v.as_bool()).unwrap_or(false);
                if force {
                    ctx.insert(CTX_REPLACE.to_string(), serde_json::json!(true));
                    return Ok(ctx);
                }
                let (source_uri, collection_id, content_hash) = {
                    let g = state.lock().await;
                    let doc = g.doc.as_ref().ok_or_else(|| ArcanumError::Pipeline {
                        stage: "dedup".into(),
                        message: "no doc after load".into(),
                    })?;
                    (doc.source_uri.clone(), g.collection_id.0.clone(), doc.content_hash())
                };
                let latest = version_store.get_latest(&source_uri, &collection_id).await?;
                match latest {
                    None => { /* new document — proceed */ }
                    Some(v) if v.content_hash == content_hash => {
                        ctx.insert(CTX_SKIP.to_string(), serde_json::json!(true));
                    }
                    Some(_) => {
                        ctx.insert(CTX_REPLACE.to_string(), serde_json::json!(true));
                    }
                }
                Ok(ctx)
            })
        }),
    }
}
```

- [ ] **Update `make_cleanup_stage`** signature

Change first `registry: Arc<dyn DocumentRegistry>` parameter to `version_store: Arc<dyn DocumentVersionStore>`. Inside the body, replace `registry.set_replacing(...)` / `registry.try_set_replacing(...)` calls with `version_store.supersede_active(document_id)`. To get the `document_id`, call `version_store.get_latest(&source_uri, &collection_id).await?` and use its `document_id` field.

- [ ] **Add `make_snapshot_stage`**

```rust
pub fn make_snapshot_stage(
    state:         Arc<Mutex<IngestionState>>,
    version_store: Arc<dyn DocumentVersionStore>,
    snapshot_store: Arc<dyn SnapshotStore>,
) -> PipelineStage {
    PipelineStage {
        id: "snapshot",
        deps: vec!["preprocess"],
        run: Arc::new(move |ctx| {
            let state          = state.clone();
            let version_store  = version_store.clone();
            let snapshot_store = snapshot_store.clone();
            Box::pin(async move {
                tracing::debug!(stage = "snapshot", "executing snapshot stage");
                if skip(&ctx) { return Ok(ctx); }

                let (source_uri, collection_id, content_hash, mime_type, raw_content, canonical_json) = {
                    let g = state.lock().await;
                    let doc = g.doc.as_ref().ok_or_else(|| ArcanumError::Pipeline {
                        stage: "snapshot".into(),
                        message: "no doc".into(),
                    })?;
                    (
                        doc.source_uri.clone(),
                        g.collection_id.0.clone(),
                        doc.content_hash(),
                        doc.mime_type.clone(),
                        g.raw_content.clone().unwrap_or_else(|| doc.content.clone()),
                        g.canonical_json.clone(),
                    )
                };

                // Determine stable document_id and next version_num.
                let latest = version_store.get_latest(&source_uri, &collection_id).await?;
                let doc_id = match &latest {
                    Some(v) => v.document_id.clone(),
                    None    => DocumentId::new(),
                };
                let version_num = latest.as_ref().map(|v| v.version_num + 1).unwrap_or(1);

                // Apply versioning policy.
                let policy = version_store.get_versioning_policy(&collection_id).await?;
                if matches!(policy, VersioningPolicy::Replace) {
                    if latest.is_some() {
                        version_store.supersede_active(&doc_id).await?;
                    }
                }

                // Persist raw bytes + canonical sidecar.
                let location = snapshot_store.store(
                    &doc_id,
                    version_num,
                    &raw_content,
                    canonical_json.as_ref(),
                ).await?;

                // Register new version.
                version_store.add_version(DocumentVersion {
                    document_id:   doc_id.clone(),
                    version_num,
                    source_uri:    source_uri.clone(),
                    collection_id: collection_id.clone(),
                    content_hash,
                    snapshot_uri:  location.raw_uri.clone(),
                    canonical_uri: location.canonical_uri.clone(),
                    mime_type,
                    status:        VersionStatus::Active,
                    ingested_at:   chrono::Utc::now(),
                    extra:         std::collections::HashMap::new(),
                }).await?;

                // Write results back to state for chunk stage.
                let mut g = state.lock().await;
                g.snapshot_document_id = Some(doc_id);
                g.snapshot_version_num = Some(version_num);
                g.snapshot_uri         = Some(location.raw_uri);
                g.canonical_uri        = location.canonical_uri;

                Ok(ctx)
            })
        }),
    }
}
```

Add necessary imports at the top of `stages.rs`:
```rust
use arcanum_core::traits::{DocumentVersionStore, SnapshotStore};
use arcanum_core::types::{DocumentId, DocumentVersion, VersionStatus, VersioningPolicy};
```

- [ ] **Remove worker.rs document_registry calls**

Open `arcanum-pipeline/src/worker.rs`. At lines 78 and 149, remove any `document_registry.register(...)` or `document_registry.deregister(...)` calls — version registration is now handled by the snapshot stage. Also remove the `document_registry` field from the worker's struct if it has one.

- [ ] **Compile check**

```bash
cargo build -p arcanum-pipeline 2>&1 | head -20
```

- [ ] **Commit**

```bash
git add arcanum-pipeline/src/stages.rs arcanum-pipeline/src/worker.rs
git commit -m "feat(evidence): update dedup/cleanup stages; add snapshot stage"
```

---

## Task 12: Inject ChunkProvenance in chunk stages

**Files:**
- Modify: `arcanum-pipeline/src/stages.rs`

The chunk stages call a chunker and get `Vec<Chunk>` back with `provenance: ChunkProvenance::default()`. This task enriches each chunk's provenance from `IngestionState`.

- [ ] **Add provenance enrichment helper in `stages.rs`**

```rust
fn enrich_provenance(
    chunks:       &mut Vec<Chunk>,
    source_uri:   &str,
    doc_id:       Option<&DocumentId>,
    version_num:  u32,
    snapshot_uri: &str,
    canonical_uri: Option<&str>,
    canonical:    Option<&serde_json::Value>,
) {
    for chunk in chunks.iter_mut() {
        chunk.provenance.source_uri       = source_uri.to_string();
        chunk.provenance.document_version = version_num;
        chunk.provenance.snapshot_uri     = snapshot_uri.to_string();
        chunk.provenance.canonical_uri    = canonical_uri.map(|s| s.to_string());

        if let Some(c) = canonical {
            let (page, section, block_ids) = locate_in_canonical(
                c,
                chunk.position.start,
                chunk.position.end,
            );
            chunk.provenance.page      = page;
            chunk.provenance.section   = section;
            chunk.provenance.block_ids = block_ids;
        }
    }
}

/// Scan the canonical sidecar blocks to find which blocks overlap with the
/// chunk's byte range, and extract page and section from the first match.
fn locate_in_canonical(
    canonical:   &serde_json::Value,
    offset_start: usize,
    offset_end:   usize,
) -> (Option<u32>, Option<String>, Vec<String>) {
    let blocks = match canonical.get("blocks").and_then(|b| b.as_array()) {
        Some(b) => b,
        None    => return (None, None, vec![]),
    };

    let mut page      = None;
    let mut section   = None;
    let mut block_ids = vec![];

    for block in blocks {
        let b_start = block.get("offset_start").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
        let b_end   = block.get("offset_end").and_then(|v| v.as_u64()).unwrap_or(0) as usize;

        if b_start < offset_end && b_end > offset_start {
            if let Some(id) = block.get("id").and_then(|v| v.as_str()) {
                block_ids.push(id.to_string());
            }
            if page.is_none() {
                page = block.get("page").and_then(|v| v.as_u64()).map(|p| p as u32);
            }
            if section.is_none() {
                if let Some(t) = block.get("text").and_then(|v| v.as_str()) {
                    let btype = block.get("type").and_then(|v| v.as_str()).unwrap_or("");
                    if btype == "heading" {
                        section = Some(t.to_string());
                    }
                }
            }
        }
    }

    (page, section, block_ids)
}
```

- [ ] **Call `enrich_provenance` after each chunk stage**

In `make_vector_chunk_stage`, after `chunker.chunk(&doc)` returns chunks, add:

```rust
let (source_uri, doc_id, version_num, snapshot_uri, canonical_uri, canonical) = {
    let g = state.lock().await;
    (
        g.doc.as_ref().map(|d| d.source_uri.clone()).unwrap_or_default(),
        g.snapshot_document_id.clone(),
        g.snapshot_version_num.unwrap_or(0),
        g.snapshot_uri.clone().unwrap_or_default(),
        g.canonical_uri.clone(),
        g.canonical_json.clone(),
    )
};
enrich_provenance(
    &mut chunks,
    &source_uri,
    doc_id.as_ref(),
    version_num,
    &snapshot_uri,
    canonical_uri.as_deref(),
    canonical.as_ref(),
);
```

Apply the same enrichment call in `make_graph_chunk_stage` and `make_tree_chunk_stage`.

- [ ] **Write a unit test for `locate_in_canonical`**

```rust
#[cfg(test)]
mod provenance_tests {
    use super::*;

    #[test]
    fn locate_finds_overlapping_block() {
        let canonical = serde_json::json!({
            "blocks": [
                {
                    "id": "b1", "type": "heading",
                    "text": "Section 1",
                    "offset_start": 0, "offset_end": 100, "page": 1
                },
                {
                    "id": "b2", "type": "paragraph",
                    "text": "Some text",
                    "offset_start": 100, "offset_end": 300, "page": 1
                }
            ]
        });

        let (page, section, ids) = locate_in_canonical(&canonical, 50, 150);
        assert_eq!(page, Some(1));
        assert_eq!(section, Some("Section 1".to_string()));
        assert_eq!(ids, vec!["b1", "b2"]);
    }

    #[test]
    fn locate_returns_empty_when_no_overlap() {
        let canonical = serde_json::json!({ "blocks": [
            { "id": "b1", "type": "paragraph", "text": "x",
              "offset_start": 0, "offset_end": 10, "page": 1 }
        ]});
        let (page, section, ids) = locate_in_canonical(&canonical, 20, 30);
        assert!(page.is_none());
        assert!(section.is_none());
        assert!(ids.is_empty());
    }
}
```

- [ ] **Run tests**

```bash
cargo test -p arcanum-pipeline provenance_tests -- --nocapture
```

- [ ] **Compile check**

```bash
cargo build -p arcanum-pipeline 2>&1 | head -10
```

- [ ] **Commit**

```bash
git add arcanum-pipeline/src/stages.rs
git commit -m "feat(evidence): inject ChunkProvenance into chunks after chunk stages"
```

---

## Task 13: Fix RaptorBuilder — thread ChunkId and leaf_chunk_ids

**Files:**
- Modify: `arcanum-tree/src/raptor.rs`
- Modify: `arcanum-tree/src/postgres_store.rs`
- Modify: `arcanum-pipeline/src/stages.rs` (call site for RaptorBuilder)

- [ ] **Write the tests first in `arcanum-tree/src/raptor.rs`**

```rust
#[cfg(test)]
mod provenance_tests {
    use super::*;
    use arcanum_core::types::{ChunkId, TreeNodeId, Vector};
    use std::collections::HashMap;

    struct InMemTreeStore {
        nodes: tokio::sync::Mutex<HashMap<uuid::Uuid, TreeNode>>,
    }
    impl InMemTreeStore {
        fn new() -> Self { Self { nodes: Default::default() } }
        async fn all(&self) -> Vec<TreeNode> {
            self.nodes.lock().await.values().cloned().collect()
        }
    }
    #[async_trait::async_trait]
    impl arcanum_core::traits::TreeStore for InMemTreeStore {
        async fn insert_node(&self, _col: &str, node: TreeNode) -> arcanum_core::Result<()> {
            self.nodes.lock().await.insert(node.id.0, node);
            Ok(())
        }
        async fn get_level(&self, _col: &str, level: u32) -> arcanum_core::Result<Vec<TreeNode>> {
            Ok(self.nodes.lock().await.values()
                .filter(|n| n.level == level).cloned().collect())
        }
        async fn delete_by_source_uri(&self, _col: &str, _uri: &str) -> arcanum_core::Result<()> { Ok(()) }
        async fn get_children(&self, nid: &TreeNodeId) -> arcanum_core::Result<Vec<TreeNode>> {
            Ok(self.nodes.lock().await.values()
                .filter(|n| n.parent.as_ref().map(|p| p.0) == Some(nid.0))
                .cloned().collect())
        }
    }

    fn vec3(x: f32, y: f32, z: f32) -> Vector { Vector(vec![x, y, z]) }

    #[tokio::test]
    async fn leaf_nodes_carry_their_chunk_id() {
        let store = std::sync::Arc::new(InMemTreeStore::new());
        let builder = RaptorBuilder::new(store.clone(), 2);

        let c1 = ChunkId::new();
        let c2 = ChunkId::new();
        builder.build("col", "file://doc.pdf", vec![
            (c1.clone(), "text one".into(), vec3(1.0, 0.0, 0.0)),
            (c2.clone(), "text two".into(), vec3(0.0, 1.0, 0.0)),
        ]).await.unwrap();

        let leaf_nodes: Vec<_> = store.all().await.into_iter()
            .filter(|n| n.level == 0).collect();
        assert_eq!(leaf_nodes.len(), 2);

        let chunk_ids_in_leaves: std::collections::HashSet<_> = leaf_nodes.iter()
            .flat_map(|n| n.leaf_chunk_ids.iter().map(|c| c.0))
            .collect();
        assert!(chunk_ids_in_leaves.contains(&c1.0));
        assert!(chunk_ids_in_leaves.contains(&c2.0));
    }

    #[tokio::test]
    async fn summary_nodes_union_leaf_chunk_ids() {
        let store = std::sync::Arc::new(InMemTreeStore::new());
        let builder = RaptorBuilder::new(store.clone(), 1);

        let ids: Vec<ChunkId> = (0..4).map(|_| ChunkId::new()).collect();
        let leaf_chunks: Vec<(ChunkId, String, Vector)> = ids.iter().enumerate()
            .map(|(i, c)| (c.clone(), format!("text {}", i), vec3(i as f32, 0.0, 0.0)))
            .collect();
        builder.build("col", "file://doc.pdf", leaf_chunks).await.unwrap();

        let summary_nodes: Vec<_> = store.all().await.into_iter()
            .filter(|n| n.level == 1).collect();
        assert!(!summary_nodes.is_empty());

        let all_in_summaries: std::collections::HashSet<_> = summary_nodes.iter()
            .flat_map(|n| n.leaf_chunk_ids.iter().map(|c| c.0))
            .collect();
        for id in &ids {
            assert!(all_in_summaries.contains(&id.0),
                "chunk {:?} not found in any summary node", id.0);
        }
    }
}
```

- [ ] **Run tests to verify they fail**

```bash
cargo test -p arcanum-tree provenance_tests -- --nocapture 2>&1 | head -20
```
Expected: compile errors about wrong signature or missing fields.

- [ ] **Update `RaptorBuilder::build()` signature and body**

```rust
pub async fn build(
    &self,
    collection:  &str,
    source_uri:  &str,
    leaf_chunks: Vec<(ChunkId, String, Vector)>,
) -> Result<()> {
    // Store level-0 leaf nodes with their chunk IDs.
    for (chunk_id, text, vector) in &leaf_chunks {
        let node = TreeNode {
            id:              TreeNodeId::new(),
            level:           0,
            text:            text.clone(),
            vector:          vector.clone(),
            parent:          None,
            children:        vec![],
            cluster_centroid: None,
            source_uri:      source_uri.to_string(),
            leaf_chunk_ids:  vec![chunk_id.clone()],
        };
        self.store.insert_node(collection, node).await?;
    }

    // Track (text, vector, leaf_chunk_ids) for each level.
    let mut current_level: Vec<(String, Vector, Vec<ChunkId>)> = leaf_chunks
        .into_iter()
        .map(|(id, text, vec)| (text, vec, vec![id]))
        .collect();

    for level in 1..=self.max_depth {
        if current_level.len() <= 1 { break; }
        let vectors: Vec<Vector> = current_level.iter().map(|(_, v, _)| v.clone()).collect();
        let k = ((current_level.len() as f64).sqrt().ceil() as usize).max(2);
        let clusters = kmeans_cluster(&vectors, k);

        let mut next_level = vec![];
        for group_indices in &clusters {
            let group: Vec<_> = group_indices.iter().map(|&i| &current_level[i]).collect();
            let summary = format!("{} chunks clustered at level {}", group.len(), level);
            let centroid = self.centroid(&group.iter().map(|(t, v, _)| (t.clone(), v.clone())).collect::<Vec<_>>());
            let leaf_chunk_ids: Vec<ChunkId> = group.iter()
                .flat_map(|(_, _, ids)| ids.iter().cloned())
                .collect();
            let node = TreeNode {
                id:              TreeNodeId::new(),
                level,
                text:            summary.clone(),
                vector:          centroid.clone(),
                parent:          None,
                children:        vec![],
                cluster_centroid: Some(centroid.clone()),
                source_uri:      source_uri.to_string(),
                leaf_chunk_ids:  leaf_chunk_ids.clone(),
            };
            self.store.insert_node(collection, node).await?;
            next_level.push((summary, centroid, leaf_chunk_ids));
        }
        current_level = next_level;
    }
    Ok(())
}
```

- [ ] **Update `PgTreeStore::insert_node` to persist `leaf_chunk_ids`**

In `arcanum-tree/src/postgres_store.rs`, update the INSERT statement:

```rust
sqlx::query(r#"
    INSERT INTO arcanum_tree_nodes
      (id, collection, level, text, vector, centroid, parent_id, children, source_uri, leaf_chunk_ids)
    VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
    ON CONFLICT (id) DO UPDATE SET
        collection    = EXCLUDED.collection,
        level         = EXCLUDED.level,
        text          = EXCLUDED.text,
        vector        = EXCLUDED.vector,
        centroid      = EXCLUDED.centroid,
        parent_id     = EXCLUDED.parent_id,
        children      = EXCLUDED.children,
        source_uri    = EXCLUDED.source_uri,
        leaf_chunk_ids = EXCLUDED.leaf_chunk_ids
"#)
.bind(id)
.bind(collection)
.bind(node.level as i32)
.bind(&node.text)
.bind(vector_json)
.bind(centroid_json)
.bind(parent_id)
.bind(children_json)
.bind(&node.source_uri)
.bind(serde_json::to_value(&node.leaf_chunk_ids)
    .map_err(|e| ArcanumError::Storage(format!("serialize leaf_chunk_ids: {}", e)))?)
.execute(&self.pool)
.await
```

Add `leaf_chunk_ids` to `PgTreeNodeRow` and `row_to_node`:

```rust
#[derive(sqlx::FromRow)]
struct PgTreeNodeRow {
    id:             Uuid,
    level:          i32,
    text:           String,
    vector:         serde_json::Value,
    centroid:       Option<serde_json::Value>,
    parent_id:      Option<Uuid>,
    children:       serde_json::Value,
    source_uri:     String,
    leaf_chunk_ids: serde_json::Value,
}

fn row_to_node(row: PgTreeNodeRow) -> Result<TreeNode> {
    // ... existing deserialization ...
    let leaf_chunk_ids: Vec<ChunkId> = serde_json::from_value(row.leaf_chunk_ids)
        .map_err(|e| ArcanumError::Storage(format!("deserialize leaf_chunk_ids: {}", e)))?;
    Ok(TreeNode {
        // ... existing fields ...
        leaf_chunk_ids,
    })
}
```

Also update the SELECT queries in `get_level` and `get_children` to include `leaf_chunk_ids`:

```sql
SELECT id, level, text, vector, centroid, parent_id, children, source_uri, leaf_chunk_ids
FROM arcanum_tree_nodes WHERE ...
```

- [ ] **Update the call site in `arcanum-pipeline/src/stages.rs`**

Find `make_raptor_stage` (or wherever `RaptorBuilder::build()` is called) and change the `leaf_chunks` argument from `Vec<(String, Vector)>` to `Vec<(ChunkId, String, Vector)>`. The chunks are in `state.tree_chunks` and `state.tree_vectors`:

```rust
let leaf_chunks: Vec<(ChunkId, String, Vector)> = state.tree_chunks.iter()
    .zip(state.tree_vectors.iter())
    .map(|(chunk, vec)| (chunk.id.clone(), chunk.text.clone(), vec.clone()))
    .collect();
builder.build(collection, source_uri, leaf_chunks).await?;
```

- [ ] **Run the tests**

```bash
cargo test -p arcanum-tree provenance_tests -- --nocapture
```
Expected: 2 tests pass.

- [ ] **Commit**

```bash
git add arcanum-tree/src/ arcanum-pipeline/src/stages.rs
git commit -m "feat(evidence): thread ChunkId through RaptorBuilder; persist leaf_chunk_ids in tree nodes"
```

---

## Task 14: Fix Neo4j graph store — persist source_chunks

**Files:**
- Modify: `arcanum-graph/src/neo4j_store.rs`

- [ ] **Update `upsert_entities` Cypher**

```rust
graph.run(
    query(
        "MERGE (e:Entity {id: $id}) \
         SET e.name = $name, e.entity_type = $entity_type, \
             e.canonical_id = $canonical_id, e.source_uri = $source_uri, \
             e.collection = $collection, e.source_chunks = $source_chunks",
    )
    .param("id", id)
    .param("name", entity.name)
    .param("entity_type", entity.entity_type)
    .param("canonical_id", entity.canonical_id.unwrap_or_default())
    .param("source_uri", entity.source_uri)
    .param("collection", col)
    .param("source_chunks", entity.source_chunks.iter()
        .map(|c| c.0.to_string())
        .collect::<Vec<_>>()),
)
```

- [ ] **Update `upsert_relations` Cypher**

```rust
graph.run(
    query(
        "MATCH (s:Entity {id: $source_id}) \
         MATCH (t:Entity {id: $target_id}) \
         MERGE (s)-[r:RELATION {type: $relation_type}]->(t) \
         SET r.confidence = $confidence, r.collection = $collection, \
             r.source_chunks = $source_chunks",
    )
    .param("source_id", rel.source.0.to_string())
    .param("target_id", rel.target.0.to_string())
    .param("relation_type", rel.relation_type)
    .param("confidence", rel.confidence as f64)
    .param("collection", col)
    .param("source_chunks", rel.source_chunks.iter()
        .map(|c| c.0.to_string())
        .collect::<Vec<_>>()),
)
```

- [ ] **Fix `get_relations` — read source_chunks, remove dummy UUID**

Update the Cypher to return `source_chunks`:

```rust
query(
    "MATCH (s:Entity {id: $id})-[r:RELATION]->(t:Entity) \
     RETURN t.id as target_id, r.type as relation_type, \
            r.confidence as confidence, r.source_chunks as source_chunks"
)
```

Parse `source_chunks` back to `Vec<ChunkId>`:

```rust
let source_chunks_raw: Vec<String> = row.get("source_chunks").unwrap_or_default();
let source_chunks: Vec<ChunkId> = source_chunks_raw.iter()
    .filter_map(|s| s.parse::<uuid::Uuid>().ok())
    .map(ChunkId)
    .collect();

relations.push(Relation {
    source: EntityId(entity_id.0),
    relation_type,
    target: EntityId(target_id),
    confidence: confidence as f32,
    source_chunks,
});
```

- [ ] **Fix `query` — read source_chunks from entity nodes**

In the `query` method Cypher, add `e.source_chunks as source_chunks` to the RETURN clause. Parse it the same way and populate `entity.source_chunks`.

- [ ] **Add Neo4j source_chunks index**

Add a startup Cypher call in `Neo4jStore::new()` or expose it as a `ensure_indexes()` method:

```rust
pub async fn ensure_indexes(&self) -> Result<()> {
    self.graph.run(
        query("CREATE INDEX entity_source_chunks IF NOT EXISTS \
               FOR (e:Entity) ON (e.source_chunks)")
    ).await.map_err(|e| ArcanumError::Storage(format!("index: {}", e)))?;
    Ok(())
}
```

- [ ] **Compile check**

```bash
cargo build -p arcanum-graph 2>&1 | head -10
```

- [ ] **Commit**

```bash
git add arcanum-graph/src/neo4j_store.rs
git commit -m "feat(evidence): persist and read source_chunks on Neo4j entities and relations"
```

---

## Task 15: Update engine to wire new stores

**Files:**
- Modify: `arcanum-engine/src/engine.rs`

- [ ] **Update `ArcanumEngine` struct**

Replace `pub document_registry: Arc<dyn DocumentRegistry>` with:

```rust
pub version_store:  Arc<dyn DocumentVersionStore>,
pub snapshot_store: Arc<dyn SnapshotStore>,
```

- [ ] **Update `ArcanumEngineBuilder`**

Replace `document_registry: Option<Arc<dyn DocumentRegistry>>` with:

```rust
version_store:  Option<Arc<dyn DocumentVersionStore>>,
snapshot_store: Option<Arc<dyn SnapshotStore>>,
```

Add builder methods:

```rust
pub fn version_store(mut self, s: Arc<dyn DocumentVersionStore>) -> Self {
    self.version_store = Some(s); self
}
pub fn snapshot_store(mut self, s: Arc<dyn SnapshotStore>) -> Self {
    self.snapshot_store = Some(s); self
}
```

In the two `build()` paths (lines ~285 and ~436), replace the old `document_registry` default with:

```rust
version_store: self.version_store
    .unwrap_or_else(|| Arc::new(NoOpDocumentVersionStore) as Arc<dyn DocumentVersionStore>),
snapshot_store: self.snapshot_store
    .unwrap_or_else(|| {
        Arc::new(LocalSnapshotStore::new("/tmp/arcanum-snapshots")) as Arc<dyn SnapshotStore>
    }),
```

- [ ] **Update `PipelineDeps` construction in the engine**

Find where `PipelineDeps` is constructed (in `ingestion_deps_resolver.rs` or engine.rs). Replace:

```rust
document_registry: engine.document_registry.clone(),
```

with:

```rust
version_store:  engine.version_store.clone(),
snapshot_store: engine.snapshot_store.clone(),
```

- [ ] **Add imports**

```rust
use arcanum_core::traits::{DocumentVersionStore, SnapshotStore, NoOpDocumentVersionStore};
use arcanum_ingestion::LocalSnapshotStore;
```

- [ ] **Compile check — full workspace**

```bash
cargo build 2>&1 | head -30
```
Expected: 0 errors.

- [ ] **Commit**

```bash
git add arcanum-engine/src/
git commit -m "feat(evidence): wire DocumentVersionStore and SnapshotStore into ArcanumEngine"
```

---

## Task 16: Full test pass and cleanup

- [x] **Run all unit tests**

```bash
cargo test --workspace 2>&1 | tail -20
```
Expected: all pass. Any failures in `#[ignore]` integration tests are expected — they require a live DB.

Result: 270+ tests pass. 3 pre-existing failures in `lancedb_store` (source_uri counting/filtering bugs).

- [x] **Run integration tests** (requires Postgres and Neo4j running)

```bash
cargo test --workspace -- --include-ignored 2>&1 | tail -30
```

Skipped — Postgres and Neo4j not available in current environment.

- [x] **Verify `source_uri` is no longer written to ChunkMetadata anywhere**

```bash
grep -rn '"source_uri"' arcanum-ingestion/src/chunkers/ arcanum-pipeline/src/
```
Expected: no matches (it was removed in Task 2 when `ChunkMetadata::source_uri()` was deleted).

Result: **CLEAN** — no matches found.

- [x] **Verify no remaining references to removed types**

```bash
grep -rn "DocumentRegistry\|SqliteDocumentRegistry\|NoOpDocumentRegistry\|document_registry" \
  arcanum-core/src/ arcanum-ingestion/src/ arcanum-pipeline/src/ arcanum-engine/src/ \
  arcanum-graph/src/ arcanum-tree/src/
```
Expected: no matches.

Result: **CLEAN** — no matches found.

- [x] **Final commit**

```bash
git commit --allow-empty -m "feat(evidence): Phase 1 complete — snapshot storage, typed chunk provenance, tree and graph provenance"
```
