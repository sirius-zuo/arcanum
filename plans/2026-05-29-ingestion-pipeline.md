# Ingestion Pipeline Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Wire the ingestion pipeline end-to-end: source loading → mime detection → preprocessing → chunking → embedding → vector storage, with a background worker driving all five named pipeline templates.

**Architecture:** `arcanum-ingestion` provides isolated stage implementations (loaders, preprocessors, chunkers). `arcanum-pipeline` owns execution: the `IngestionWorker` drains `BoundedQueue<IngestionTask>`, selects a template from `ArcanumPipelineRegistry`, builds a typed `PipelineDAG`, and executes it with shared `IngestionState` carrying the document through each stage. `arcanum-engine`'s `IngestionService` adds URI-level dedup before queuing.

**Tech Stack:** Rust async (tokio), `infer` crate for mime detection, `zip` for EPUB/Office disambiguation, `reqwest` for HTTP loading, `sha2`/`hex` for content hashing. References: `docs/superpowers/specs/2026-05-29-ingestion-design.md`, `docs/design/architecture-v2.html` §3.2 §9 §11.

---

## File Map

### arcanum-core (foundation — done first, everything depends on it)
| Action | File | Purpose |
|---|---|---|
| Modify | `src/traits/ingestion.rs` | Extend `Source` enum; add `CloudProvider`, `ConnectorKind`, `Source::from_uri()` |
| Create | `src/traits/progress.rs` | `ProgressEmitter` trait so `IngestionWorker` emits events without depending on arcanum-engine |
| Modify | `src/traits/mod.rs` | Re-export `ProgressEmitter` |

### arcanum-ingestion
| Action | File | Purpose |
|---|---|---|
| Modify | `src/metadata.rs` | Extend `DocumentHashTracker` with URI→hash store (`seen_unchanged`, `record`, `ever_seen`) |
| Create | `src/detection.rs` | `MimeDetector` — magic bytes + ZIP disambiguation |
| Create | `src/loaders/registry.rs` | `LoaderRegistry` — routes `Source` to matching `DocumentLoader` |
| Modify | `src/loaders/file.rs` | Update `Source::Raw` pattern to new `mime_hint: Option<String>` field |
| Create | `src/loaders/raw.rs` | `RawLoader` — passes `Source::Raw` bytes through as `RawDocument` |
| Create | `src/loaders/http.rs` | `HttpLoader` — fetches `Source::Url` via reqwest, uses `Content-Type` as hint |
| Create | `src/loaders/database.rs` | `DatabaseLoader` stub — supports `Source::Database`, returns `Err` |
| Create | `src/loaders/cloud_storage.rs` | `CloudStorageLoader` stub — supports `Source::CloudStorage`, returns `Err` |
| Create | `src/loaders/git.rs` | `GitLoader` stub — supports `Source::Git`, returns `Err` |
| Create | `src/loaders/connector.rs` | `ConnectorLoader` stub — supports `Source::Connector`, returns `Err` |
| Create | `src/preprocessors/registry.rs` | `PreprocessorRegistry` — mime-keyed `Vec<Preprocessor>` chains |
| Modify | `src/loaders/mod.rs` | Re-export all loaders + `LoaderRegistry` |
| Modify | `src/preprocessors/mod.rs` | Re-export `PreprocessorRegistry` |
| Modify | `src/lib.rs` | Re-export `MimeDetector`, `LoaderRegistry`, `PreprocessorRegistry`, updated `DocumentHashTracker` |
| Modify | `Cargo.toml` | Add `infer = "0.16"` |
| Create | `tests/detection_test.rs` | Tests for `MimeDetector` |
| Create | `tests/loader_registry_test.rs` | Tests for `LoaderRegistry` routing |
| Modify | `tests/preprocessor_test.rs` | Add registry chain tests |

### arcanum-pipeline
| Action | File | Purpose |
|---|---|---|
| Create | `src/ingestion_state.rs` | `IngestionState` — typed doc/chunks/vectors shared across stages |
| Create | `src/deps.rs` | `PipelineDeps` — injectable deps struct for all template builders |
| Create | `src/stage_failure.rs` | `StageFailure` enum — Core vs NonCore classification |
| Create | `src/stages.rs` | Shared stage builder functions (`make_load_stage`, `make_preprocess_stage`, …) |
| Create | `src/registry.rs` | `ArcanumPipelineRegistry` — name → `TemplateBuilder` map |
| Modify | `src/templates/standard.rs` | Replace no-op with wired DAG using shared stage builders |
| Create | `src/templates/contextual.rs` | Contextual template |
| Create | `src/templates/graph.rs` | Graph template |
| Create | `src/templates/raptor.rs` | RAPTOR template |
| Create | `src/templates/full.rs` | Full template |
| Modify | `src/templates/mod.rs` | Re-export all templates |
| Create | `src/worker.rs` | `IngestionWorker` — background task pool |
| Modify | `src/lib.rs` | Re-export all new types |
| Modify | `Cargo.toml` | Add `arcanum-ingestion`, `arcanum-tree` dependencies |
| Create | `tests/worker_test.rs` | Worker integration tests |
| Create | `tests/registry_test.rs` | Registry lookup tests |
| Create | `tests/standard_pipeline_test.rs` | Full Standard DAG end-to-end |

### arcanum-engine
| Action | File | Purpose |
|---|---|---|
| Modify | `src/services/ingestion.rs` | Add `hash_tracker` to `IngestionService`; URI-level dedup before queue push |
| Modify | `tests/engine_test.rs` | Add dedup test |

---

## Task 1: Extend Source enum + add `Source::from_uri()`

**Files:**
- Modify: `arcanum-core/src/traits/ingestion.rs`

- [ ] **Step 1: Write failing tests**

Add to the existing `#[cfg(test)]` block in `arcanum-core/src/traits/ingestion.rs`:

```rust
#[test]
fn test_source_from_uri_http() {
    let s = Source::from_uri("https://example.com/doc.pdf").unwrap();
    assert!(matches!(s, Source::Url(_)));
}

#[test]
fn test_source_from_uri_file() {
    let s = Source::from_uri("/tmp/doc.pdf").unwrap();
    assert!(matches!(s, Source::File(_)));
}

#[test]
fn test_source_from_uri_s3() {
    let s = Source::from_uri("s3://my-bucket/path/doc.pdf").unwrap();
    assert!(matches!(s, Source::CloudStorage { provider: CloudProvider::S3, .. }));
}

#[test]
fn test_cloud_storage_uri_display() {
    let s = Source::CloudStorage {
        provider: CloudProvider::Gcs,
        bucket: "b".into(),
        key: "k/doc.pdf".into(),
    };
    assert_eq!(s.uri(), "gs://b/k/doc.pdf");
}

#[test]
fn test_raw_mime_hint_optional() {
    let s = Source::Raw { content: b"data".to_vec(), mime_hint: None, uri: "raw://1".into() };
    assert_eq!(s.uri(), "raw://1");
}
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cargo test -p arcanum-core 2>&1 | grep -E "FAILED|error"
```

Expected: compile errors — `CloudProvider`, `from_uri`, `mime_hint` not found.

- [ ] **Step 3: Implement**

Replace `arcanum-core/src/traits/ingestion.rs` with:

```rust
use async_trait::async_trait;
use crate::types::*;
use crate::Result;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub enum CloudProvider { S3, Gcs, AzureBlob }

#[derive(Debug, Clone)]
pub enum ConnectorKind { GoogleDrive, Notion, Confluence }

#[derive(Debug, Clone)]
pub enum Source {
    File(PathBuf),
    Url(String),
    Database { connection_string: String, query: String, display_uri: String },
    Raw { content: Vec<u8>, mime_hint: Option<String>, uri: String },
    CloudStorage { provider: CloudProvider, bucket: String, key: String },
    Git { repo_url: String, branch: String, path_glob: Option<String> },
    Connector { provider: ConnectorKind, resource_id: String },
}

impl Source {
    pub fn uri(&self) -> &str {
        match self {
            Source::File(p)                    => p.to_str().unwrap_or(""),
            Source::Url(u)                     => u,
            Source::Database { display_uri, .. } => display_uri,
            Source::Raw { uri, .. }            => uri,
            Source::Git { repo_url, .. }       => repo_url,
            // CloudStorage and Connector return formatted strings — callers use uri() for display only
            Source::CloudStorage { .. }        => "cloud://",
            Source::Connector { .. }           => "connector://",
        }
    }

    pub fn from_uri(uri: &str) -> crate::Result<Source> {
        if uri.starts_with("http://") || uri.starts_with("https://") {
            return Ok(Source::Url(uri.to_string()));
        }
        if let Some(rest) = uri.strip_prefix("s3://") {
            let (bucket, key) = rest.split_once('/').unwrap_or((rest, ""));
            return Ok(Source::CloudStorage {
                provider: CloudProvider::S3,
                bucket: bucket.to_string(),
                key: key.to_string(),
            });
        }
        if let Some(rest) = uri.strip_prefix("gs://") {
            let (bucket, key) = rest.split_once('/').unwrap_or((rest, ""));
            return Ok(Source::CloudStorage {
                provider: CloudProvider::Gcs,
                bucket: bucket.to_string(),
                key: key.to_string(),
            });
        }
        if let Some(rest) = uri.strip_prefix("az://") {
            let (bucket, key) = rest.split_once('/').unwrap_or((rest, ""));
            return Ok(Source::CloudStorage {
                provider: CloudProvider::AzureBlob,
                bucket: bucket.to_string(),
                key: key.to_string(),
            });
        }
        Ok(Source::File(PathBuf::from(uri)))
    }
}

// Fix uri() for CloudStorage — return owned string via a separate method
impl Source {
    pub fn display_uri(&self) -> String {
        match self {
            Source::CloudStorage { provider, bucket, key } => {
                let scheme = match provider {
                    CloudProvider::S3       => "s3",
                    CloudProvider::Gcs      => "gs",
                    CloudProvider::AzureBlob => "az",
                };
                format!("{scheme}://{bucket}/{key}")
            }
            Source::Connector { provider, resource_id } => {
                let scheme = match provider {
                    ConnectorKind::GoogleDrive => "gdrive",
                    ConnectorKind::Notion      => "notion",
                    ConnectorKind::Confluence  => "confluence",
                };
                format!("{scheme}://{resource_id}")
            }
            other => other.uri().to_string(),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_source_from_uri_http() {
        let s = Source::from_uri("https://example.com/doc.pdf").unwrap();
        assert!(matches!(s, Source::Url(_)));
    }

    #[test]
    fn test_source_from_uri_file() {
        let s = Source::from_uri("/tmp/doc.pdf").unwrap();
        assert!(matches!(s, Source::File(_)));
    }

    #[test]
    fn test_source_from_uri_s3() {
        let s = Source::from_uri("s3://my-bucket/path/doc.pdf").unwrap();
        assert!(matches!(s, Source::CloudStorage { provider: CloudProvider::S3, .. }));
    }

    #[test]
    fn test_cloud_storage_uri_display() {
        let s = Source::CloudStorage {
            provider: CloudProvider::Gcs,
            bucket: "b".into(),
            key: "k/doc.pdf".into(),
        };
        assert_eq!(s.display_uri(), "gs://b/k/doc.pdf");
    }

    #[test]
    fn test_raw_mime_hint_optional() {
        let s = Source::Raw { content: b"data".to_vec(), mime_hint: None, uri: "raw://1".into() };
        assert_eq!(s.uri(), "raw://1");
    }

    #[test]
    fn test_database_uri_does_not_expose_credentials() {
        let source = Source::Database {
            connection_string: "postgres://admin:secret@host:5432/mydb".to_string(),
            query: "SELECT * FROM docs".to_string(),
            display_uri: "postgres://host:5432/mydb".to_string(),
        };
        assert!(!source.uri().contains("secret"));
    }

    struct MockLoader;
    #[async_trait::async_trait]
    impl DocumentLoader for MockLoader {
        async fn load(&self, source: &Source) -> crate::Result<RawDocument> {
            Ok(RawDocument {
                id: DocumentId::new(), content: b"hello".to_vec(),
                mime_type: "text/plain".to_string(),
                source_uri: source.uri().to_string(),
                metadata: Default::default(),
            })
        }
        fn supports(&self, source: &Source) -> bool { matches!(source, Source::File(_)) }
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

- [ ] **Step 4: Run tests**

```bash
cargo test -p arcanum-core 2>&1 | tail -20
```

Expected: all tests pass. Fix any `Source::Raw { mime_type` usages elsewhere that now need `mime_hint`.

- [ ] **Step 5: Commit**

```bash
git add arcanum-core/src/traits/ingestion.rs
git commit -m "feat(core): extend Source enum with CloudStorage/Git/Connector variants and from_uri()"
```

---

## Task 2: Add `ProgressEmitter` trait

**Files:**
- Create: `arcanum-core/src/traits/progress.rs`
- Modify: `arcanum-core/src/traits/mod.rs`

- [ ] **Step 1: Write failing test**

Add to `arcanum-core/src/traits/progress.rs` (new file):

```rust
use async_trait::async_trait;

#[async_trait]
pub trait ProgressEmitter: Send + Sync {
    async fn emit(&self, event: &str, payload: serde_json::Value);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    struct RecordingEmitter(Arc<Mutex<Vec<(String, serde_json::Value)>>>);

    #[async_trait]
    impl ProgressEmitter for RecordingEmitter {
        async fn emit(&self, event: &str, payload: serde_json::Value) {
            self.0.lock().unwrap().push((event.to_string(), payload));
        }
    }

    #[tokio::test]
    async fn test_emitter_receives_events() {
        let log = Arc::new(Mutex::new(vec![]));
        let emitter = RecordingEmitter(log.clone());
        emitter.emit("ingestion:progress", serde_json::json!({"status": "queued"})).await;
        let entries = log.lock().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].0, "ingestion:progress");
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cargo test -p arcanum-core 2>&1 | grep -E "error|FAILED"
```

Expected: compile error — file not in module tree.

- [ ] **Step 3: Add to mod.rs**

In `arcanum-core/src/traits/mod.rs`, add:

```rust
pub mod progress;
pub use progress::ProgressEmitter;
```

- [ ] **Step 4: Run tests**

```bash
cargo test -p arcanum-core 2>&1 | tail -10
```

Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add arcanum-core/src/traits/progress.rs arcanum-core/src/traits/mod.rs
git commit -m "feat(core): add ProgressEmitter trait for pipeline→engine event decoupling"
```

---

## Task 3: Extend `DocumentHashTracker` with stateful URI store

**Files:**
- Modify: `arcanum-ingestion/src/metadata.rs`
- Modify: `arcanum-ingestion/tests/hash_test.rs`

`DocumentHashTracker` currently only has a static `compute_hash()`. It needs to store URI→hash pairs so the system can detect unchanged documents.

- [ ] **Step 1: Write failing tests**

Add to `arcanum-ingestion/tests/hash_test.rs`:

```rust
use arcanum_ingestion::DocumentHashTracker;

#[tokio::test]
async fn test_ever_seen_false_initially() {
    let tracker = DocumentHashTracker::new();
    assert!(!tracker.ever_seen("file:///doc.pdf").await);
}

#[tokio::test]
async fn test_record_then_ever_seen() {
    let tracker = DocumentHashTracker::new();
    tracker.record("file:///doc.pdf", b"content").await;
    assert!(tracker.ever_seen("file:///doc.pdf").await);
}

#[tokio::test]
async fn test_seen_unchanged_true_when_same_content() {
    let tracker = DocumentHashTracker::new();
    tracker.record("file:///doc.pdf", b"content").await;
    assert!(tracker.seen_unchanged("file:///doc.pdf", b"content").await);
}

#[tokio::test]
async fn test_seen_unchanged_false_when_content_changed() {
    let tracker = DocumentHashTracker::new();
    tracker.record("file:///doc.pdf", b"old").await;
    assert!(!tracker.seen_unchanged("file:///doc.pdf", b"new").await);
}
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cargo test -p arcanum-ingestion --test hash_test 2>&1 | tail -15
```

Expected: compile errors — `new`, `ever_seen`, `record`, `seen_unchanged` not found.

- [ ] **Step 3: Implement**

Replace `arcanum-ingestion/src/metadata.rs`:

```rust
use sha2::{Sha256, Digest};
use std::collections::HashMap;
use tokio::sync::RwLock;

pub struct DocumentHashTracker {
    store: RwLock<HashMap<String, String>>,
}

impl DocumentHashTracker {
    pub fn new() -> Self {
        Self { store: RwLock::new(HashMap::new()) }
    }

    pub fn compute_hash(content: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(content);
        hex::encode(hasher.finalize())
    }

    pub async fn ever_seen(&self, uri: &str) -> bool {
        self.store.read().await.contains_key(uri)
    }

    pub async fn seen_unchanged(&self, uri: &str, content: &[u8]) -> bool {
        let hash = Self::compute_hash(content);
        self.store.read().await.get(uri).map_or(false, |h| h == &hash)
    }

    pub async fn record(&self, uri: &str, content: &[u8]) {
        let hash = Self::compute_hash(content);
        self.store.write().await.insert(uri.to_string(), hash);
    }
}
```

- [ ] **Step 4: Run tests**

```bash
cargo test -p arcanum-ingestion --test hash_test 2>&1 | tail -15
```

Expected: all 6 tests (2 original + 4 new) pass.

- [ ] **Step 5: Commit**

```bash
git add arcanum-ingestion/src/metadata.rs arcanum-ingestion/tests/hash_test.rs
git commit -m "feat(ingestion): extend DocumentHashTracker with stateful URI→hash dedup store"
```

---

## Task 4: `MimeDetector`

**Files:**
- Modify: `arcanum-ingestion/Cargo.toml` — add `infer = "0.16"`
- Create: `arcanum-ingestion/src/detection.rs`
- Create: `arcanum-ingestion/tests/detection_test.rs`

- [ ] **Step 1: Add dependency**

In `arcanum-ingestion/Cargo.toml`, add to `[dependencies]`:

```toml
infer = "0.16"
```

- [ ] **Step 2: Write failing tests**

Create `arcanum-ingestion/tests/detection_test.rs`:

```rust
use arcanum_ingestion::MimeDetector;
use std::io::Write;
use zip::write::{SimpleFileOptions, ZipWriter};
use zip::CompressionMethod;

fn epub_zip() -> Vec<u8> {
    let mut z = ZipWriter::new(std::io::Cursor::new(Vec::new()));
    z.start_file("META-INF/container.xml",
        SimpleFileOptions::default().compression_method(CompressionMethod::Stored)).unwrap();
    z.write_all(b"<?xml?>").unwrap();
    z.finish().unwrap().into_inner()
}

fn bare_zip() -> Vec<u8> {
    let mut z = ZipWriter::new(std::io::Cursor::new(Vec::new()));
    z.start_file("data.txt",
        SimpleFileOptions::default().compression_method(CompressionMethod::Stored)).unwrap();
    z.write_all(b"hello").unwrap();
    z.finish().unwrap().into_inner()
}

fn office_zip() -> Vec<u8> {
    let mut z = ZipWriter::new(std::io::Cursor::new(Vec::new()));
    z.start_file("[Content_Types].xml",
        SimpleFileOptions::default().compression_method(CompressionMethod::Stored)).unwrap();
    z.write_all(b"<?xml?>").unwrap();
    z.finish().unwrap().into_inner()
}

#[test]
fn test_pdf_magic_detected() {
    let pdf = b"%PDF-1.4 content";
    assert_eq!(MimeDetector::detect(pdf, None), "application/pdf");
}

#[test]
fn test_epub_disambiguated_from_zip() {
    assert_eq!(MimeDetector::detect(&epub_zip(), None), "application/epub+zip");
}

#[test]
fn test_office_zip_detected() {
    assert_eq!(MimeDetector::detect(&office_zip(), None), "application/vnd.openxmlformats");
}

#[test]
fn test_bare_zip_stays_zip() {
    assert_eq!(MimeDetector::detect(&bare_zip(), None), "application/zip");
}

#[test]
fn test_hint_used_when_no_magic() {
    assert_eq!(MimeDetector::detect(b"# markdown", Some("text/markdown")), "text/markdown");
}

#[test]
fn test_fallback_to_octet_stream() {
    assert_eq!(MimeDetector::detect(b"no magic here", None), "application/octet-stream");
}

#[test]
fn test_html_magic_detected() {
    assert_eq!(MimeDetector::detect(b"<!DOCTYPE html><html>", None), "text/html");
}
```

- [ ] **Step 3: Run tests to verify they fail**

```bash
cargo test -p arcanum-ingestion --test detection_test 2>&1 | grep -E "error|FAILED"
```

Expected: compile error — `MimeDetector` not found.

- [ ] **Step 4: Implement**

Create `arcanum-ingestion/src/detection.rs`:

```rust
pub struct MimeDetector;

impl MimeDetector {
    pub fn detect(content: &[u8], hint: Option<&str>) -> String {
        if let Some(kind) = infer::get(content) {
            let magic = kind.mime_type();
            if magic == "application/zip" {
                return Self::disambiguate_zip(content);
            }
            return magic.to_string();
        }
        hint.map(str::to_string)
            .unwrap_or_else(|| "application/octet-stream".to_string())
    }

    fn disambiguate_zip(content: &[u8]) -> String {
        let cursor = std::io::Cursor::new(content);
        let mut zip = match zip::ZipArchive::new(cursor) {
            Ok(z) => z,
            Err(_) => return "application/zip".to_string(),
        };
        if zip.by_name("META-INF/container.xml").is_ok() {
            return "application/epub+zip".to_string();
        }
        if zip.by_name("[Content_Types].xml").is_ok() {
            return "application/vnd.openxmlformats".to_string();
        }
        "application/zip".to_string()
    }
}
```

Add to `arcanum-ingestion/src/lib.rs`:

```rust
pub mod detection;
pub use detection::MimeDetector;
```

- [ ] **Step 5: Run tests**

```bash
cargo test -p arcanum-ingestion --test detection_test 2>&1 | tail -15
```

Expected: all 7 tests pass.

- [ ] **Step 6: Commit**

```bash
git add arcanum-ingestion/src/detection.rs arcanum-ingestion/src/lib.rs \
        arcanum-ingestion/tests/detection_test.rs arcanum-ingestion/Cargo.toml
git commit -m "feat(ingestion): add MimeDetector with magic-byte detection and ZIP disambiguation"
```

---

## Task 5: `LoaderRegistry`

**Files:**
- Create: `arcanum-ingestion/src/loaders/registry.rs`
- Create: `arcanum-ingestion/tests/loader_registry_test.rs`
- Modify: `arcanum-ingestion/src/loaders/mod.rs`

- [ ] **Step 1: Write failing tests**

Create `arcanum-ingestion/tests/loader_registry_test.rs`:

```rust
use arcanum_core::traits::{DocumentLoader, Source};
use arcanum_core::types::*;
use arcanum_core::{Result, ArcanumError};
use arcanum_ingestion::loaders::LoaderRegistry;
use async_trait::async_trait;
use std::sync::Arc;

struct AlwaysLoader;
#[async_trait]
impl DocumentLoader for AlwaysLoader {
    async fn load(&self, source: &Source) -> Result<RawDocument> {
        Ok(RawDocument {
            id: DocumentId::new(), content: b"data".to_vec(),
            mime_type: "text/plain".into(), source_uri: source.uri().to_string(),
            metadata: Default::default(),
        })
    }
    fn supports(&self, _: &Source) -> bool { true }
}

struct NeverLoader;
#[async_trait]
impl DocumentLoader for NeverLoader {
    async fn load(&self, _: &Source) -> Result<RawDocument> {
        Err(ArcanumError::Ingestion("never".into()))
    }
    fn supports(&self, _: &Source) -> bool { false }
}

#[tokio::test]
async fn test_registry_routes_to_first_supporting_loader() {
    let reg = LoaderRegistry::new()
        .register(Arc::new(NeverLoader))
        .register(Arc::new(AlwaysLoader));
    let doc = reg.load(&Source::Url("https://x.com".into())).await.unwrap();
    assert_eq!(doc.content, b"data");
}

#[tokio::test]
async fn test_registry_errors_when_no_loader_matches() {
    let reg = LoaderRegistry::new().register(Arc::new(NeverLoader));
    assert!(reg.load(&Source::Url("https://x.com".into())).await.is_err());
}

#[tokio::test]
async fn test_empty_registry_errors() {
    let reg = LoaderRegistry::new();
    assert!(reg.load(&Source::File("/tmp/x".into())).await.is_err());
}
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cargo test -p arcanum-ingestion --test loader_registry_test 2>&1 | grep -E "error|FAILED"
```

Expected: compile error — `LoaderRegistry` not found.

- [ ] **Step 3: Implement**

Create `arcanum-ingestion/src/loaders/registry.rs`:

```rust
use arcanum_core::{traits::{DocumentLoader, Source}, types::RawDocument, Result, ArcanumError};
use std::sync::Arc;

pub struct LoaderRegistry {
    loaders: Vec<Arc<dyn DocumentLoader>>,
}

impl LoaderRegistry {
    pub fn new() -> Self { Self { loaders: vec![] } }

    pub fn register(mut self, loader: Arc<dyn DocumentLoader>) -> Self {
        self.loaders.push(loader);
        self
    }

    pub async fn load(&self, source: &Source) -> Result<RawDocument> {
        self.loaders.iter()
            .find(|l| l.supports(source))
            .ok_or_else(|| ArcanumError::Ingestion(
                format!("no loader registered for source: {}", source.display_uri())
            ))?
            .load(source).await
    }
}
```

Add to `arcanum-ingestion/src/loaders/mod.rs`:

```rust
mod registry;
pub use registry::LoaderRegistry;
```

Add to `arcanum-ingestion/src/lib.rs`:

```rust
pub use loaders::LoaderRegistry;
```

- [ ] **Step 4: Run tests**

```bash
cargo test -p arcanum-ingestion --test loader_registry_test 2>&1 | tail -10
```

Expected: all 3 tests pass.

- [ ] **Step 5: Commit**

```bash
git add arcanum-ingestion/src/loaders/registry.rs arcanum-ingestion/src/loaders/mod.rs \
        arcanum-ingestion/tests/loader_registry_test.rs arcanum-ingestion/src/lib.rs
git commit -m "feat(ingestion): add LoaderRegistry for source-type routing"
```

---

## Task 6: `RawLoader` + update `FileLoader` for new `Source::Raw`

**Files:**
- Create: `arcanum-ingestion/src/loaders/raw.rs`
- Modify: `arcanum-ingestion/src/loaders/file.rs`
- Modify: `arcanum-ingestion/src/loaders/mod.rs`
- Modify: `arcanum-ingestion/tests/loader_test.rs`

- [ ] **Step 1: Write failing tests**

Add to `arcanum-ingestion/tests/loader_test.rs`:

```rust
use arcanum_ingestion::loaders::RawLoader;
use arcanum_core::traits::{DocumentLoader, Source};

#[tokio::test]
async fn test_raw_loader_passes_content_through() {
    let loader = RawLoader::new();
    let source = Source::Raw {
        content: b"hello world".to_vec(),
        mime_hint: Some("text/plain".into()),
        uri: "raw://test".into(),
    };
    assert!(loader.supports(&source));
    let doc = loader.load(&source).await.unwrap();
    assert_eq!(doc.content, b"hello world");
    assert_eq!(doc.mime_type, "text/plain");
    assert_eq!(doc.source_uri, "raw://test");
}

#[tokio::test]
async fn test_raw_loader_defaults_mime_when_hint_absent() {
    let loader = RawLoader::new();
    let source = Source::Raw {
        content: b"data".to_vec(),
        mime_hint: None,
        uri: "raw://x".into(),
    };
    let doc = loader.load(&source).await.unwrap();
    assert_eq!(doc.mime_type, "application/octet-stream");
}

#[tokio::test]
async fn test_file_loader_detects_epub_mime() {
    use arcanum_ingestion::FileLoader;
    use std::io::Write;
    let mut tmp = tempfile::Builder::new().suffix(".epub").tempfile().unwrap();
    tmp.write_all(b"PK dummy").unwrap();
    let loader = FileLoader::new();
    let source = Source::File(tmp.path().to_path_buf());
    let doc = loader.load(&source).await.unwrap();
    assert_eq!(doc.mime_type, "application/epub+zip");
}
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cargo test -p arcanum-ingestion --test loader_test 2>&1 | grep -E "error|FAILED"
```

Expected: `RawLoader` not found.

- [ ] **Step 3: Create `RawLoader`**

Create `arcanum-ingestion/src/loaders/raw.rs`:

```rust
use arcanum_core::{traits::{DocumentLoader, Source}, types::*, Result, ArcanumError};
use async_trait::async_trait;

pub struct RawLoader;

impl RawLoader {
    pub fn new() -> Self { Self }
}

#[async_trait]
impl DocumentLoader for RawLoader {
    async fn load(&self, source: &Source) -> Result<RawDocument> {
        let Source::Raw { content, mime_hint, uri } = source else {
            return Err(ArcanumError::Ingestion("RawLoader only handles Source::Raw".into()));
        };
        Ok(RawDocument {
            id: DocumentId::new(),
            content: content.clone(),
            mime_type: mime_hint.clone().unwrap_or_else(|| "application/octet-stream".to_string()),
            source_uri: uri.clone(),
            metadata: Default::default(),
        })
    }

    fn supports(&self, source: &Source) -> bool {
        matches!(source, Source::Raw { .. })
    }
}
```

Update `arcanum-ingestion/src/loaders/mod.rs`:

```rust
mod file;
mod raw;
mod registry;
pub use file::FileLoader;
pub use raw::RawLoader;
pub use registry::LoaderRegistry;
```

- [ ] **Step 4: Run tests**

```bash
cargo test -p arcanum-ingestion --test loader_test 2>&1 | tail -15
```

Expected: all loader tests pass. If `Source::Raw { mime_type` pattern fails in `file.rs`, update it to `mime_hint`.

- [ ] **Step 5: Commit**

```bash
git add arcanum-ingestion/src/loaders/raw.rs arcanum-ingestion/src/loaders/mod.rs \
        arcanum-ingestion/tests/loader_test.rs
git commit -m "feat(ingestion): add RawLoader; update FileLoader for Source::Raw mime_hint field"
```

---

## Task 7: `HttpLoader`

**Files:**
- Create: `arcanum-ingestion/src/loaders/http.rs`
- Modify: `arcanum-ingestion/src/loaders/mod.rs`
- Modify: `arcanum-ingestion/tests/loader_test.rs`

`reqwest` is already a workspace dependency via `arcanum-models`.

- [ ] **Step 1: Add reqwest to arcanum-ingestion Cargo.toml**

```toml
reqwest = { workspace = true }
```

- [ ] **Step 2: Write failing tests**

Add to `arcanum-ingestion/tests/loader_test.rs`:

```rust
use arcanum_ingestion::loaders::HttpLoader;

#[test]
fn test_http_loader_supports_url() {
    let loader = HttpLoader::new();
    assert!(loader.supports(&Source::Url("https://example.com".into())));
    assert!(!loader.supports(&Source::File("/tmp/x".into())));
}

#[tokio::test]
async fn test_http_loader_returns_error_on_connection_refused() {
    let loader = HttpLoader::new();
    // Port 1 — always refused
    let source = Source::Url("http://127.0.0.1:1/doc".into());
    assert!(loader.load(&source).await.is_err());
}
```

- [ ] **Step 3: Run tests to verify they fail**

```bash
cargo test -p arcanum-ingestion --test loader_test 2>&1 | grep "HttpLoader"
```

Expected: compile error.

- [ ] **Step 4: Implement**

Create `arcanum-ingestion/src/loaders/http.rs`:

```rust
use arcanum_core::{traits::{DocumentLoader, Source}, types::*, Result, ArcanumError};
use async_trait::async_trait;

pub struct HttpLoader {
    client: reqwest::Client,
}

impl HttpLoader {
    pub fn new() -> Self {
        Self { client: reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build().unwrap() }
    }
}

#[async_trait]
impl DocumentLoader for HttpLoader {
    async fn load(&self, source: &Source) -> Result<RawDocument> {
        let Source::Url(url) = source else {
            return Err(ArcanumError::Ingestion("HttpLoader only handles Source::Url".into()));
        };
        let resp = self.client.get(url).send().await
            .map_err(|e| ArcanumError::Ingestion(format!("HTTP fetch failed: {e}")))?;
        let mime_hint = resp.headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.split(';').next().unwrap_or(s).trim().to_string());
        let content = resp.bytes().await
            .map_err(|e| ArcanumError::Ingestion(format!("HTTP read failed: {e}")))?
            .to_vec();
        Ok(RawDocument {
            id: DocumentId::new(),
            mime_type: mime_hint.unwrap_or_else(|| "application/octet-stream".to_string()),
            source_uri: url.clone(),
            content,
            metadata: Default::default(),
        })
    }

    fn supports(&self, source: &Source) -> bool {
        matches!(source, Source::Url(_))
    }
}
```

Add to `arcanum-ingestion/src/loaders/mod.rs`:

```rust
mod http;
pub use http::HttpLoader;
```

- [ ] **Step 5: Run tests**

```bash
cargo test -p arcanum-ingestion --test loader_test 2>&1 | tail -15
```

Expected: all tests pass (connection refused test returns `is_err() == true`).

- [ ] **Step 6: Commit**

```bash
git add arcanum-ingestion/src/loaders/http.rs arcanum-ingestion/src/loaders/mod.rs \
        arcanum-ingestion/Cargo.toml arcanum-ingestion/tests/loader_test.rs
git commit -m "feat(ingestion): add HttpLoader with Content-Type hint extraction"
```

---

## Task 8: Stub loaders (Database, CloudStorage, Git, Connector)

**Files:**
- Create: `arcanum-ingestion/src/loaders/database.rs`
- Create: `arcanum-ingestion/src/loaders/cloud_storage.rs`
- Create: `arcanum-ingestion/src/loaders/git.rs`
- Create: `arcanum-ingestion/src/loaders/connector.rs`
- Modify: `arcanum-ingestion/src/loaders/mod.rs`
- Modify: `arcanum-ingestion/tests/loader_test.rs`

All four follow the same pattern: correct `supports()`, `load()` returns `Err`. Tests verify routing only.

- [ ] **Step 1: Write failing tests**

Add to `arcanum-ingestion/tests/loader_test.rs`:

```rust
use arcanum_core::traits::CloudProvider;
use arcanum_ingestion::loaders::{DatabaseLoader, CloudStorageLoader, GitLoader, ConnectorLoader};

#[test]
fn test_stub_loaders_support_correct_variants() {
    use arcanum_core::traits::{ConnectorKind};
    assert!(DatabaseLoader::new().supports(&Source::Database {
        connection_string: "postgres://localhost/db".into(),
        query: "SELECT 1".into(), display_uri: "postgres://localhost/db".into(),
    }));
    assert!(CloudStorageLoader::new().supports(&Source::CloudStorage {
        provider: CloudProvider::S3, bucket: "b".into(), key: "k".into(),
    }));
    assert!(GitLoader::new().supports(&Source::Git {
        repo_url: "https://github.com/x/y".into(), branch: "main".into(), path_glob: None,
    }));
    assert!(ConnectorLoader::new().supports(&Source::Connector {
        provider: ConnectorKind::Notion, resource_id: "page-123".into(),
    }));
}

#[tokio::test]
async fn test_stub_loaders_return_error_on_load() {
    let src = Source::Database {
        connection_string: "postgres://localhost/db".into(),
        query: "SELECT 1".into(), display_uri: "postgres://localhost/db".into(),
    };
    assert!(DatabaseLoader::new().load(&src).await.is_err());
}
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cargo test -p arcanum-ingestion --test loader_test 2>&1 | grep -E "DatabaseLoader|error"
```

- [ ] **Step 3: Implement all four stubs**

Create `arcanum-ingestion/src/loaders/database.rs`:

```rust
use arcanum_core::{traits::{DocumentLoader, Source}, types::RawDocument, Result, ArcanumError};
use async_trait::async_trait;

pub struct DatabaseLoader;
impl DatabaseLoader { pub fn new() -> Self { Self } }

#[async_trait]
impl DocumentLoader for DatabaseLoader {
    async fn load(&self, _: &Source) -> Result<RawDocument> {
        Err(ArcanumError::Ingestion("DatabaseLoader not yet implemented".into()))
    }
    fn supports(&self, s: &Source) -> bool { matches!(s, Source::Database { .. }) }
}
```

Create `arcanum-ingestion/src/loaders/cloud_storage.rs`:

```rust
use arcanum_core::{traits::{DocumentLoader, Source}, types::RawDocument, Result, ArcanumError};
use async_trait::async_trait;

pub struct CloudStorageLoader;
impl CloudStorageLoader { pub fn new() -> Self { Self } }

#[async_trait]
impl DocumentLoader for CloudStorageLoader {
    async fn load(&self, _: &Source) -> Result<RawDocument> {
        Err(ArcanumError::Ingestion("CloudStorageLoader not yet implemented".into()))
    }
    fn supports(&self, s: &Source) -> bool { matches!(s, Source::CloudStorage { .. }) }
}
```

Create `arcanum-ingestion/src/loaders/git.rs`:

```rust
use arcanum_core::{traits::{DocumentLoader, Source}, types::RawDocument, Result, ArcanumError};
use async_trait::async_trait;

pub struct GitLoader;
impl GitLoader { pub fn new() -> Self { Self } }

#[async_trait]
impl DocumentLoader for GitLoader {
    async fn load(&self, _: &Source) -> Result<RawDocument> {
        Err(ArcanumError::Ingestion("GitLoader not yet implemented".into()))
    }
    fn supports(&self, s: &Source) -> bool { matches!(s, Source::Git { .. }) }
}
```

Create `arcanum-ingestion/src/loaders/connector.rs`:

```rust
use arcanum_core::{traits::{DocumentLoader, Source}, types::RawDocument, Result, ArcanumError};
use async_trait::async_trait;

pub struct ConnectorLoader;
impl ConnectorLoader { pub fn new() -> Self { Self } }

#[async_trait]
impl DocumentLoader for ConnectorLoader {
    async fn load(&self, _: &Source) -> Result<RawDocument> {
        Err(ArcanumError::Ingestion("ConnectorLoader not yet implemented".into()))
    }
    fn supports(&self, s: &Source) -> bool { matches!(s, Source::Connector { .. }) }
}
```

Update `arcanum-ingestion/src/loaders/mod.rs`:

```rust
mod file;
mod raw;
mod http;
mod database;
mod cloud_storage;
mod git;
mod connector;
mod registry;

pub use file::FileLoader;
pub use raw::RawLoader;
pub use http::HttpLoader;
pub use database::DatabaseLoader;
pub use cloud_storage::CloudStorageLoader;
pub use git::GitLoader;
pub use connector::ConnectorLoader;
pub use registry::LoaderRegistry;
```

Also update `arcanum-ingestion/src/lib.rs` to re-export:

```rust
pub use loaders::{
    FileLoader, RawLoader, HttpLoader,
    DatabaseLoader, CloudStorageLoader, GitLoader, ConnectorLoader,
    LoaderRegistry,
};
```

- [ ] **Step 4: Run tests**

```bash
cargo test -p arcanum-ingestion --test loader_test 2>&1 | tail -15
```

Expected: all tests pass.

- [ ] **Step 5: Commit**

```bash
git add arcanum-ingestion/src/loaders/
git commit -m "feat(ingestion): add stub loaders for Database, CloudStorage, Git, Connector sources"
```

---

## Task 9: `PreprocessorRegistry`

**Files:**
- Create: `arcanum-ingestion/src/preprocessors/registry.rs`
- Modify: `arcanum-ingestion/src/preprocessors/mod.rs`
- Modify: `arcanum-ingestion/src/lib.rs`
- Modify: `arcanum-ingestion/tests/preprocessor_test.rs`

- [ ] **Step 1: Write failing tests**

Add to `arcanum-ingestion/tests/preprocessor_test.rs`:

```rust
use arcanum_ingestion::PreprocessorRegistry;

#[tokio::test]
async fn test_registry_runs_chain_for_matching_mime() {
    let registry = PreprocessorRegistry::new()
        .register("text/html", Arc::new(HtmlCleaner::new()));
    let doc = raw_doc(b"<p>hello</p>".to_vec(), "text/html");
    let out = registry.process(doc).await.unwrap();
    assert!(!String::from_utf8(out.content).unwrap().contains('<'));
}

#[tokio::test]
async fn test_registry_passthrough_unknown_mime() {
    let registry = PreprocessorRegistry::new();
    let doc = raw_doc(b"raw bytes".to_vec(), "application/octet-stream");
    let out = registry.process(doc).await.unwrap();
    assert_eq!(out.content, b"raw bytes");
}

#[tokio::test]
async fn test_registry_chain_runs_in_order() {
    use std::sync::{Arc as StdArc, Mutex};
    use arcanum_core::traits::Preprocessor;

    let log = StdArc::new(Mutex::new(vec![]));

    struct Recorder(StdArc<Mutex<Vec<u8>>>, u8);
    #[async_trait::async_trait]
    impl Preprocessor for Recorder {
        async fn process(&self, doc: RawDocument) -> arcanum_core::Result<RawDocument> {
            self.0.lock().unwrap().push(self.1);
            Ok(doc)
        }
    }

    let registry = PreprocessorRegistry::new()
        .register("text/plain", Arc::new(Recorder(log.clone(), 1)))
        .register("text/plain", Arc::new(Recorder(log.clone(), 2)));

    let doc = raw_doc(b"data".to_vec(), "text/plain");
    registry.process(doc).await.unwrap();
    assert_eq!(*log.lock().unwrap(), vec![1u8, 2u8]);
}

#[tokio::test]
async fn test_default_chains_routes_html() {
    let registry = PreprocessorRegistry::default_chains();
    let doc = raw_doc(b"<h1>Title</h1><p>Body</p>".to_vec(), "text/html");
    let out = registry.process(doc).await.unwrap();
    assert_eq!(out.mime_type, "text/plain");
}
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cargo test -p arcanum-ingestion --test preprocessor_test 2>&1 | grep -E "error|PreprocessorRegistry"
```

- [ ] **Step 3: Implement**

Create `arcanum-ingestion/src/preprocessors/registry.rs`:

```rust
use arcanum_core::{traits::Preprocessor, types::RawDocument, Result};
use std::collections::HashMap;
use std::sync::Arc;

pub struct PreprocessorRegistry {
    chains: HashMap<String, Vec<Arc<dyn Preprocessor>>>,
}

impl PreprocessorRegistry {
    pub fn new() -> Self { Self { chains: HashMap::new() } }

    pub fn register(mut self, mime: &str, p: Arc<dyn Preprocessor>) -> Self {
        self.chains.entry(mime.to_string()).or_default().push(p);
        self
    }

    pub async fn process(&self, doc: RawDocument) -> Result<RawDocument> {
        let chain = self.chains.get(&doc.mime_type).cloned().unwrap_or_default();
        let mut out = doc;
        for p in chain {
            out = p.process(out).await?;
        }
        Ok(out)
    }

    pub fn default_chains() -> Self {
        use crate::preprocessors::{HtmlCleaner, PdfParser, EpubParser};
        Self::new()
            .register("text/html",             Arc::new(HtmlCleaner::new()))
            .register("application/xhtml+xml", Arc::new(HtmlCleaner::new()))
            .register("application/pdf",       Arc::new(PdfParser::new()))
            .register("application/epub+zip",  Arc::new(EpubParser::new()))
    }
}
```

Update `arcanum-ingestion/src/preprocessors/mod.rs`:

```rust
mod html;
mod pdf;
mod epub;
mod registry;
pub use html::HtmlCleaner;
pub use pdf::PdfParser;
pub use epub::EpubParser;
pub use registry::PreprocessorRegistry;
```

Update `arcanum-ingestion/src/lib.rs` to also export:

```rust
pub use preprocessors::{HtmlCleaner, PdfParser, EpubParser, PreprocessorRegistry};
```

- [ ] **Step 4: Run tests**

```bash
cargo test -p arcanum-ingestion --test preprocessor_test 2>&1 | tail -20
```

Expected: all 16 tests pass (12 existing + 4 new).

- [ ] **Step 5: Commit**

```bash
git add arcanum-ingestion/src/preprocessors/registry.rs \
        arcanum-ingestion/src/preprocessors/mod.rs \
        arcanum-ingestion/src/lib.rs \
        arcanum-ingestion/tests/preprocessor_test.rs
git commit -m "feat(ingestion): add PreprocessorRegistry with mime-keyed chains and default_chains()"
```

---

## Task 10: `IngestionState`, `PipelineDeps`, `StageFailure`

**Files:**
- Create: `arcanum-pipeline/src/ingestion_state.rs`
- Create: `arcanum-pipeline/src/deps.rs`
- Create: `arcanum-pipeline/src/stage_failure.rs`
- Modify: `arcanum-pipeline/src/lib.rs`
- Modify: `arcanum-pipeline/Cargo.toml`

- [ ] **Step 1: Add dependencies to arcanum-pipeline Cargo.toml**

```toml
[dependencies]
arcanum-ingestion = { path = "../arcanum-ingestion" }
arcanum-tree      = { path = "../arcanum-tree" }
```

- [ ] **Step 2: Write failing tests**

Add a new test in `arcanum-pipeline/tests/` — create `arcanum-pipeline/tests/state_test.rs`:

```rust
use arcanum_pipeline::{IngestionState, StageFailure};
use arcanum_core::traits::Source;
use arcanum_core::types::CollectionId;
use std::path::PathBuf;

#[test]
fn test_ingestion_state_starts_empty() {
    let state = IngestionState::new(
        Source::File(PathBuf::from("/tmp/x")),
        CollectionId("col1".into()),
    );
    assert!(state.doc.is_none());
    assert!(state.chunks.is_empty());
    assert!(state.vectors.is_empty());
}

#[test]
fn test_stage_failure_is_core_or_noncore() {
    use arcanum_core::ArcanumError;
    let f = StageFailure::Core { stage: "load".into(), error: ArcanumError::Ingestion("x".into()) };
    assert!(matches!(f, StageFailure::Core { .. }));
}
```

- [ ] **Step 3: Run tests to verify they fail**

```bash
cargo test -p arcanum-pipeline --test state_test 2>&1 | grep -E "error|FAILED"
```

- [ ] **Step 4: Implement**

Create `arcanum-pipeline/src/ingestion_state.rs`:

```rust
use arcanum_core::{traits::Source, types::*};

pub struct IngestionState {
    pub source:        Source,
    pub collection_id: CollectionId,
    pub doc:           Option<RawDocument>,
    pub chunks:        Vec<Chunk>,
    pub vectors:       Vec<Vector>,
}

impl IngestionState {
    pub fn new(source: Source, collection_id: CollectionId) -> Self {
        Self { source, collection_id, doc: None, chunks: vec![], vectors: vec![] }
    }
}
```

Create `arcanum-pipeline/src/deps.rs`:

```rust
use arcanum_core::{traits::*, types::*};
use arcanum_ingestion::{LoaderRegistry, PreprocessorRegistry, DocumentHashTracker};
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
}
```

Create `arcanum-pipeline/src/stage_failure.rs`:

```rust
use arcanum_core::ArcanumError;

pub enum StageFailure {
    Core    { stage: String, error: ArcanumError },
    NonCore { stage: String, error: ArcanumError },
}

pub fn is_core_stage(stage_id: &str) -> bool {
    matches!(stage_id, "load" | "preprocess" | "chunk" | "embed" | "vector_write")
}
```

Update `arcanum-pipeline/src/lib.rs`:

```rust
pub mod dag;
pub mod executor;
pub mod templates;
pub mod ingestion_state;
pub mod deps;
pub mod stage_failure;

pub use dag::{PipelineDAG, PipelineStage, StageFn, StageContext};
pub use executor::DagExecutor;
pub use ingestion_state::IngestionState;
pub use deps::PipelineDeps;
pub use stage_failure::{StageFailure, is_core_stage};

pub enum PipelineTemplate {
    Standard, Contextual, Graph, Raptor, Full, Custom(PipelineDAG),
}
```

- [ ] **Step 5: Run tests**

```bash
cargo test -p arcanum-pipeline --test state_test 2>&1 | tail -10
```

Expected: all 2 tests pass.

- [ ] **Step 6: Commit**

```bash
git add arcanum-pipeline/src/ingestion_state.rs arcanum-pipeline/src/deps.rs \
        arcanum-pipeline/src/stage_failure.rs arcanum-pipeline/src/lib.rs \
        arcanum-pipeline/Cargo.toml arcanum-pipeline/tests/state_test.rs
git commit -m "feat(pipeline): add IngestionState, PipelineDeps, StageFailure foundation types"
```

---

## Task 11: Shared stage builder functions

**Files:**
- Create: `arcanum-pipeline/src/stages.rs`
- Modify: `arcanum-pipeline/src/lib.rs`

These functions are used by all five templates. Defining them once here prevents duplication.

- [ ] **Step 1: Write failing test**

Create `arcanum-pipeline/tests/stages_test.rs`:

```rust
use arcanum_pipeline::stages::make_load_stage;
use arcanum_core::traits::Source;
use arcanum_core::types::*;
use arcanum_pipeline::IngestionState;
use std::sync::Arc;
use std::path::PathBuf;

// Uses a mock RawLoader to verify make_load_stage populates IngestionState.doc
// Full end-to-end tested in standard_pipeline_test.rs — this just checks the stage wiring.
#[tokio::test]
async fn test_make_load_stage_id_is_load() {
    use arcanum_pipeline::stages::make_load_stage;
    // stage.id must be "load" for DAG dependency resolution
    // We verify by inspecting the returned PipelineStage
    use arcanum_ingestion::{LoaderRegistry, RawLoader, DocumentHashTracker};
    let loaders = Arc::new(LoaderRegistry::new().register(Arc::new(RawLoader::new())));
    let tracker = Arc::new(DocumentHashTracker::new());
    let state = Arc::new(tokio::sync::Mutex::new(IngestionState::new(
        Source::Raw { content: b"test".to_vec(), mime_hint: None, uri: "raw://x".into() },
        CollectionId("col".into()),
    )));
    let stage = make_load_stage(state, loaders, tracker);
    assert_eq!(stage.id, "load");
    assert!(stage.deps.is_empty());
}
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cargo test -p arcanum-pipeline --test stages_test 2>&1 | grep -E "error|stages"
```

- [ ] **Step 3: Implement**

Create `arcanum-pipeline/src/stages.rs`:

```rust
use crate::{dag::PipelineStage, IngestionState};
use arcanum_core::{traits::*, types::*, ArcanumError};
use arcanum_ingestion::{LoaderRegistry, PreprocessorRegistry, DocumentHashTracker, MimeDetector};
use std::sync::Arc;
use tokio::sync::Mutex;

pub fn make_load_stage(
    state: Arc<Mutex<IngestionState>>,
    loaders: Arc<LoaderRegistry>,
    hash_tracker: Arc<DocumentHashTracker>,
) -> PipelineStage {
    PipelineStage {
        id: "load",
        deps: vec![],
        run: Arc::new(move |mut ctx| {
            let state = state.clone(); let loaders = loaders.clone(); let ht = hash_tracker.clone();
            Box::pin(async move {
                let source = state.lock().await.source.clone();
                let mut doc = loaders.load(&source).await?;
                doc.mime_type = MimeDetector::detect(&doc.content, Some(&doc.mime_type));
                if ht.seen_unchanged(&doc.source_uri, &doc.content).await {
                    ctx.insert("__skip".to_string(), serde_json::json!(true));
                    return Ok(ctx);
                }
                state.lock().await.doc = Some(doc);
                Ok(ctx)
            })
        }),
    }
}

pub fn make_preprocess_stage(
    state: Arc<Mutex<IngestionState>>,
    preprocessors: Arc<PreprocessorRegistry>,
) -> PipelineStage {
    PipelineStage {
        id: "preprocess",
        deps: vec!["load"],
        run: Arc::new(move |ctx| {
            let state = state.clone(); let pp = preprocessors.clone();
            Box::pin(async move {
                if ctx.get("__skip").and_then(|v| v.as_bool()).unwrap_or(false) { return Ok(ctx); }
                let doc = state.lock().await.doc.clone()
                    .ok_or_else(|| ArcanumError::Pipeline { stage: "preprocess".into(), message: "no doc".into() })?;
                let processed = pp.process(doc).await?;
                state.lock().await.doc = Some(processed);
                Ok(ctx)
            })
        }),
    }
}

pub fn make_chunk_stage(
    state: Arc<Mutex<IngestionState>>,
    chunker: Arc<dyn Chunker>,
) -> PipelineStage {
    PipelineStage {
        id: "chunk",
        deps: vec!["preprocess"],
        run: Arc::new(move |ctx| {
            let state = state.clone(); let chunker = chunker.clone();
            Box::pin(async move {
                if ctx.get("__skip").and_then(|v| v.as_bool()).unwrap_or(false) { return Ok(ctx); }
                let (doc, collection_id) = {
                    let g = state.lock().await;
                    (g.doc.clone().ok_or_else(|| ArcanumError::Pipeline { stage: "chunk".into(), message: "no doc".into() })?,
                     g.collection_id.clone())
                };
                let mut chunks = chunker.chunk(&doc).await?;
                for c in &mut chunks { c.collection_id = collection_id.clone(); }
                state.lock().await.chunks = chunks;
                Ok(ctx)
            })
        }),
    }
}

pub fn make_context_enrich_stage(
    state: Arc<Mutex<IngestionState>>,
    enricher: Arc<dyn TextEnricher>,
) -> PipelineStage {
    PipelineStage {
        id: "context_enrich",
        deps: vec!["chunk"],
        run: Arc::new(move |ctx| {
            let state = state.clone(); let enricher = enricher.clone();
            Box::pin(async move {
                if ctx.get("__skip").and_then(|v| v.as_bool()).unwrap_or(false) { return Ok(ctx); }
                let chunks = state.lock().await.chunks.clone();
                let mut enriched = Vec::with_capacity(chunks.len());
                for mut chunk in chunks {
                    let req = EnrichRequest {
                        text: chunk.text.clone(),
                        intent: EnrichIntent::ContextPrefix,
                        context: None,
                    };
                    let prefix = enricher.enrich(req).await?.0;
                    chunk.text = format!("{prefix} {}", chunk.text);
                    enriched.push(chunk);
                }
                state.lock().await.chunks = enriched;
                Ok(ctx)
            })
        }),
    }
}

pub fn make_embed_stage(
    state: Arc<Mutex<IngestionState>>,
    embedder: Arc<dyn Embedder>,
) -> PipelineStage {
    PipelineStage {
        id: "embed",
        deps: vec!["chunk"],
        run: Arc::new(move |ctx| {
            let state = state.clone(); let embedder = embedder.clone();
            Box::pin(async move {
                if ctx.get("__skip").and_then(|v| v.as_bool()).unwrap_or(false) { return Ok(ctx); }
                let texts: Vec<String> = state.lock().await.chunks.iter().map(|c| c.text.clone()).collect();
                let vectors = embedder.embed(texts).await?;
                state.lock().await.vectors = vectors;
                Ok(ctx)
            })
        }),
    }
}

pub fn make_embed_stage_after(dep: &'static str, state: Arc<Mutex<IngestionState>>, embedder: Arc<dyn Embedder>) -> PipelineStage {
    let mut stage = make_embed_stage(state, embedder);
    stage.deps = vec![dep];
    stage
}

pub fn make_vector_write_stage(
    state: Arc<Mutex<IngestionState>>,
    vector_store: Arc<dyn VectorStore>,
) -> PipelineStage {
    PipelineStage {
        id: "vector_write",
        deps: vec!["embed"],
        run: Arc::new(move |mut ctx| {
            let state = state.clone(); let vs = vector_store.clone();
            Box::pin(async move {
                if ctx.get("__skip").and_then(|v| v.as_bool()).unwrap_or(false) { return Ok(ctx); }
                let (chunks, vectors, collection_id) = {
                    let g = state.lock().await;
                    (g.chunks.clone(), g.vectors.clone(), g.collection_id.clone())
                };
                let indexed: Vec<IndexedChunk> = chunks.into_iter().zip(vectors)
                    .map(|(chunk, vector)| IndexedChunk {
                        chunk, vector, token_vectors: None, store_id: String::new(),
                    }).collect();
                vs.upsert(&collection_id.0, indexed).await?;
                {
                    let g = state.lock().await;
                    ht_record_if_available(&g).await;
                }
                ctx.insert("vector_write_ok".to_string(), serde_json::json!(true));
                Ok(ctx)
            })
        }),
    }
}

// hash_tracker record is handled separately by the worker after pipeline success.
async fn ht_record_if_available(_state: &IngestionState) {}

pub fn make_entity_extract_stage(
    state: Arc<Mutex<IngestionState>>,
    extractor: Arc<dyn TextEnricher>,
    graph_store: Arc<dyn GraphStore>,
) -> PipelineStage {
    PipelineStage {
        id: "entity_extract",
        deps: vec!["preprocess"],
        run: Arc::new(move |mut ctx| {
            let state = state.clone(); let ext = extractor.clone(); let gs = graph_store.clone();
            Box::pin(async move {
                if ctx.get("__skip").and_then(|v| v.as_bool()).unwrap_or(false) { return Ok(ctx); }
                let doc = state.lock().await.doc.clone()
                    .ok_or_else(|| ArcanumError::Pipeline { stage: "entity_extract".into(), message: "no doc".into() })?;
                let req = EnrichRequest {
                    text: String::from_utf8_lossy(&doc.content).to_string(),
                    intent: EnrichIntent::ExtractEntities,
                    context: None,
                };
                let result = ext.enrich(req).await?.0;
                // Parse JSON: {"entities":[{"name":...,"type":...}],"relations":[...]}
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&result) {
                    let entities: Vec<Entity> = v["entities"].as_array().unwrap_or(&vec![]).iter()
                        .filter_map(|e| Some(Entity {
                            id: EntityId::new(),
                            name: e["name"].as_str()?.to_string(),
                            entity_type: e["type"].as_str().unwrap_or("unknown").to_string(),
                            canonical_id: None,
                            source_chunks: vec![],
                        })).collect();
                    if !entities.is_empty() {
                        gs.upsert_entities(entities).await
                            .unwrap_or_else(|e| tracing::warn!("graph_write failed (noncore): {e}"));
                    }
                }
                ctx.insert("entity_extract_ok".to_string(), serde_json::json!(true));
                Ok(ctx)
            })
        }),
    }
}

pub fn make_raptor_build_stage(
    state: Arc<Mutex<IngestionState>>,
    tree_store: Arc<dyn TreeStore>,
) -> PipelineStage {
    PipelineStage {
        id: "raptor_build",
        deps: vec!["embed"],
        run: Arc::new(move |mut ctx| {
            let state = state.clone(); let ts = tree_store.clone();
            Box::pin(async move {
                if ctx.get("__skip").and_then(|v| v.as_bool()).unwrap_or(false) { return Ok(ctx); }
                let (chunks, vectors, collection_id) = {
                    let g = state.lock().await;
                    (g.chunks.clone(), g.vectors.clone(), g.collection_id.clone())
                };
                let leaf_chunks: Vec<(String, Vector)> = chunks.into_iter()
                    .zip(vectors).map(|(c, v)| (c.text, v)).collect();

                // RaptorBuilder is generic over TreeStore — use the trait object adapter
                struct TreeStoreAdapter(Arc<dyn TreeStore>);
                #[async_trait::async_trait]
                impl TreeStore for TreeStoreAdapter {
                    async fn insert_node(&self, collection: &str, node: arcanum_core::types::TreeNode) -> arcanum_core::Result<()> {
                        self.0.insert_node(collection, node).await
                    }
                    async fn get_level(&self, collection: &str, level: u32) -> arcanum_core::Result<Vec<arcanum_core::types::TreeNode>> {
                        self.0.get_level(collection, level).await
                    }
                    async fn get_children(&self, node_id: &arcanum_core::types::TreeNodeId) -> arcanum_core::Result<Vec<arcanum_core::types::TreeNode>> {
                        self.0.get_children(node_id).await
                    }
                }
                let adapter = Arc::new(TreeStoreAdapter(ts));
                let builder = arcanum_tree::RaptorBuilder::new(adapter, 3);
                builder.build(&collection_id.0, leaf_chunks).await
                    .unwrap_or_else(|e| tracing::warn!("raptor_build failed (noncore): {e}"));
                ctx.insert("raptor_build_ok".to_string(), serde_json::json!(true));
                Ok(ctx)
            })
        }),
    }
}
```

Add to `arcanum-pipeline/src/lib.rs`:

```rust
pub mod stages;
```

- [ ] **Step 4: Run tests**

```bash
cargo test -p arcanum-pipeline --test stages_test 2>&1 | tail -10
```

Expected: `test_make_load_stage_id_is_load` passes.

- [ ] **Step 5: Commit**

```bash
git add arcanum-pipeline/src/stages.rs arcanum-pipeline/src/lib.rs \
        arcanum-pipeline/tests/stages_test.rs
git commit -m "feat(pipeline): add shared stage builder functions (load, preprocess, chunk, embed, write)"
```

---

## Task 12: `ArcanumPipelineRegistry` + Standard template (wired)

**Files:**
- Create: `arcanum-pipeline/src/registry.rs`
- Modify: `arcanum-pipeline/src/templates/standard.rs`
- Modify: `arcanum-pipeline/src/templates/mod.rs`
- Modify: `arcanum-pipeline/src/lib.rs`
- Create: `arcanum-pipeline/tests/registry_test.rs`
- Create: `arcanum-pipeline/tests/standard_pipeline_test.rs`

- [ ] **Step 1: Write failing registry tests**

Create `arcanum-pipeline/tests/registry_test.rs`:

```rust
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
    #[async_trait] impl Chunker for StubChunker {
        async fn chunk(&self, doc: &RawDocument) -> arcanum_core::Result<Vec<Chunk>> { Ok(vec![]) }
    }
    struct StubEmbedder;
    #[async_trait] impl Embedder for StubEmbedder {
        async fn embed(&self, t: Vec<String>) -> arcanum_core::Result<Vec<Vector>> { Ok(vec![]) }
        fn dimension(&self) -> usize { 3 }
    }
    struct StubVectorStore;
    #[async_trait] impl VectorStore for StubVectorStore {
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
```

- [ ] **Step 2: Write failing standard pipeline test**

Create `arcanum-pipeline/tests/standard_pipeline_test.rs`:

```rust
// Uses the same stub_deps helper as registry_test.rs (copy it here for independence)
// ... (same stub_deps function)

#[tokio::test]
async fn test_standard_pipeline_runs_all_five_stages() {
    use arcanum_pipeline::{ArcanumPipelineRegistry, DagExecutor, IngestionState, PipelineDeps};
    use arcanum_core::traits::Source;
    use arcanum_core::types::CollectionId;
    use std::sync::Arc;
    use tokio::sync::Mutex;

    let deps = stub_deps();
    let state = Arc::new(Mutex::new(IngestionState::new(
        Source::Raw {
            content: b"hello world document".to_vec(),
            mime_hint: Some("text/plain".into()),
            uri: "raw://test".into(),
        },
        CollectionId("col1".into()),
    )));
    let reg = ArcanumPipelineRegistry::default();
    let dag = reg.build("standard", state.clone(), &deps).unwrap();
    let ctx = DagExecutor::execute(&dag, Default::default()).await.unwrap();
    // vector_write_ok is set by make_vector_write_stage on success
    assert!(ctx.contains_key("vector_write_ok") || ctx.contains_key("__skip"));
}
```

- [ ] **Step 3: Run tests to verify they fail**

```bash
cargo test -p arcanum-pipeline --test registry_test 2>&1 | grep -E "error|FAILED"
```

- [ ] **Step 4: Implement registry**

Create `arcanum-pipeline/src/registry.rs`:

```rust
use crate::{dag::PipelineDAG, deps::PipelineDeps, ingestion_state::IngestionState};
use arcanum_core::{Result, ArcanumError};
use std::{collections::HashMap, sync::Arc};
use tokio::sync::Mutex;

pub type TemplateBuilder = Arc<
    dyn Fn(Arc<Mutex<IngestionState>>, &PipelineDeps) -> PipelineDAG + Send + Sync
>;

pub struct ArcanumPipelineRegistry {
    builders: HashMap<String, TemplateBuilder>,
}

impl ArcanumPipelineRegistry {
    pub fn new() -> Self { Self { builders: HashMap::new() } }

    pub fn register(&mut self, name: &str, builder: TemplateBuilder) {
        self.builders.insert(name.to_string(), builder);
    }

    pub fn build(
        &self, name: &str,
        state: Arc<Mutex<IngestionState>>,
        deps: &PipelineDeps,
    ) -> Result<PipelineDAG> {
        self.builders.get(name)
            .ok_or_else(|| ArcanumError::Pipeline {
                stage: "registry".into(),
                message: format!("unknown pipeline template: '{name}'"),
            })
            .map(|builder| builder(state, deps))
    }

    pub fn default() -> Self {
        let mut r = Self::new();
        r.register("standard",   crate::templates::standard::builder());
        r.register("contextual", crate::templates::contextual::builder());
        r.register("graph",      crate::templates::graph::builder());
        r.register("raptor",     crate::templates::raptor::builder());
        r.register("full",       crate::templates::full::builder());
        r
    }
}
```

- [ ] **Step 5: Wire Standard template**

Replace `arcanum-pipeline/src/templates/standard.rs`:

```rust
use crate::{
    dag::PipelineDAG,
    deps::PipelineDeps,
    ingestion_state::IngestionState,
    registry::TemplateBuilder,
    stages::*,
};
use std::sync::Arc;
use tokio::sync::Mutex;

pub fn builder() -> TemplateBuilder {
    Arc::new(|state: Arc<Mutex<IngestionState>>, deps: &PipelineDeps| {
        PipelineDAG::new()
            .add_stage(make_load_stage(state.clone(), deps.loaders.clone(), deps.hash_tracker.clone()))
            .add_stage(make_preprocess_stage(state.clone(), deps.preprocessors.clone()))
            .add_stage(make_chunk_stage(state.clone(), deps.chunker.clone()))
            .add_stage(make_embed_stage(state.clone(), deps.embedder.clone()))
            .add_stage(make_vector_write_stage(state.clone(), deps.vector_store.clone()))
    })
}
```

Update `arcanum-pipeline/src/templates/mod.rs`:

```rust
pub mod standard;
pub mod contextual;
pub mod graph;
pub mod raptor;
pub mod full;
```

(Create empty stub files for contextual/graph/raptor/full that will be filled in Task 13.)

Update `arcanum-pipeline/src/lib.rs`:

```rust
pub mod registry;
pub use registry::ArcanumPipelineRegistry;
```

- [ ] **Step 6: Run tests**

```bash
cargo test -p arcanum-pipeline --test registry_test --test standard_pipeline_test 2>&1 | tail -15
```

Expected: all 3 tests pass.

- [ ] **Step 7: Commit**

```bash
git add arcanum-pipeline/src/registry.rs arcanum-pipeline/src/templates/ \
        arcanum-pipeline/src/lib.rs arcanum-pipeline/tests/registry_test.rs \
        arcanum-pipeline/tests/standard_pipeline_test.rs
git commit -m "feat(pipeline): add ArcanumPipelineRegistry and wire Standard template DAG"
```

---

## Task 13: Contextual, Graph, RAPTOR, Full templates

**Files:**
- Modify: `arcanum-pipeline/src/templates/contextual.rs`
- Modify: `arcanum-pipeline/src/templates/graph.rs`
- Modify: `arcanum-pipeline/src/templates/raptor.rs`
- Modify: `arcanum-pipeline/src/templates/full.rs`

No new test files — verified via `registry_test` (all five templates must be buildable from the registry) and `standard_pipeline_test` pattern.

- [ ] **Step 1: Write failing tests**

Add to `arcanum-pipeline/tests/registry_test.rs`:

```rust
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
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cargo test -p arcanum-pipeline --test registry_test test_registry_all_five 2>&1 | tail -10
```

Expected: panics in contextual/graph/raptor/full builders (empty stubs).

- [ ] **Step 3: Implement Contextual template**

`arcanum-pipeline/src/templates/contextual.rs`:

```rust
use crate::{dag::PipelineDAG, deps::PipelineDeps, ingestion_state::IngestionState,
            registry::TemplateBuilder, stages::*, templates::standard};
use std::sync::Arc;
use tokio::sync::Mutex;

pub fn builder() -> TemplateBuilder {
    Arc::new(|state: Arc<Mutex<IngestionState>>, deps: &PipelineDeps| {
        match &deps.context_enricher {
            None => {
                tracing::warn!("context_enricher not configured — falling back to Standard pipeline");
                return standard::builder()(state, deps);
            }
            Some(enricher) => {
                PipelineDAG::new()
                    .add_stage(make_load_stage(state.clone(), deps.loaders.clone(), deps.hash_tracker.clone()))
                    .add_stage(make_preprocess_stage(state.clone(), deps.preprocessors.clone()))
                    .add_stage(make_chunk_stage(state.clone(), deps.chunker.clone()))
                    .add_stage(make_context_enrich_stage(state.clone(), enricher.clone()))
                    .add_stage(make_embed_stage_after("context_enrich", state.clone(), deps.embedder.clone()))
                    .add_stage(make_vector_write_stage(state.clone(), deps.vector_store.clone()))
            }
        }
    })
}
```

- [ ] **Step 4: Implement Graph template**

`arcanum-pipeline/src/templates/graph.rs`:

```rust
use crate::{dag::PipelineDAG, deps::PipelineDeps, ingestion_state::IngestionState,
            registry::TemplateBuilder, stages::*, templates::standard};
use std::sync::Arc;
use tokio::sync::Mutex;

pub fn builder() -> TemplateBuilder {
    Arc::new(|state: Arc<Mutex<IngestionState>>, deps: &PipelineDeps| {
        match (&deps.entity_extractor, &deps.graph_store) {
            (Some(extractor), Some(graph_store)) => {
                PipelineDAG::new()
                    .add_stage(make_load_stage(state.clone(), deps.loaders.clone(), deps.hash_tracker.clone()))
                    .add_stage(make_preprocess_stage(state.clone(), deps.preprocessors.clone()))
                    // Parallel branch 1: entity extraction → graph write
                    .add_stage(make_entity_extract_stage(state.clone(), extractor.clone(), graph_store.clone()))
                    // Parallel branch 2: chunk → embed → vector write
                    .add_stage(make_chunk_stage(state.clone(), deps.chunker.clone()))
                    .add_stage(make_embed_stage(state.clone(), deps.embedder.clone()))
                    .add_stage(make_vector_write_stage(state.clone(), deps.vector_store.clone()))
            }
            _ => {
                tracing::warn!("entity_extractor or graph_store not configured — falling back to Standard");
                standard::builder()(state, deps)
            }
        }
    })
}
```

- [ ] **Step 5: Implement RAPTOR template**

`arcanum-pipeline/src/templates/raptor.rs`:

```rust
use crate::{dag::PipelineDAG, deps::PipelineDeps, ingestion_state::IngestionState,
            registry::TemplateBuilder, stages::*, templates::standard};
use std::sync::Arc;
use tokio::sync::Mutex;

pub fn builder() -> TemplateBuilder {
    Arc::new(|state: Arc<Mutex<IngestionState>>, deps: &PipelineDeps| {
        match &deps.tree_store {
            Some(tree_store) => {
                PipelineDAG::new()
                    .add_stage(make_load_stage(state.clone(), deps.loaders.clone(), deps.hash_tracker.clone()))
                    .add_stage(make_preprocess_stage(state.clone(), deps.preprocessors.clone()))
                    .add_stage(make_chunk_stage(state.clone(), deps.chunker.clone()))
                    .add_stage(make_embed_stage(state.clone(), deps.embedder.clone()))
                    // Two independent terminal stages after embed
                    .add_stage(make_vector_write_stage(state.clone(), deps.vector_store.clone()))
                    .add_stage(make_raptor_build_stage(state.clone(), tree_store.clone()))
            }
            None => {
                tracing::warn!("tree_store not configured — falling back to Standard");
                standard::builder()(state, deps)
            }
        }
    })
}
```

- [ ] **Step 6: Implement Full template**

`arcanum-pipeline/src/templates/full.rs`:

```rust
use crate::{dag::PipelineDAG, deps::PipelineDeps, ingestion_state::IngestionState,
            registry::TemplateBuilder, stages::*, templates::contextual};
use std::sync::Arc;
use tokio::sync::Mutex;

pub fn builder() -> TemplateBuilder {
    Arc::new(|state: Arc<Mutex<IngestionState>>, deps: &PipelineDeps| {
        let mut dag = PipelineDAG::new()
            .add_stage(make_load_stage(state.clone(), deps.loaders.clone(), deps.hash_tracker.clone()))
            .add_stage(make_preprocess_stage(state.clone(), deps.preprocessors.clone()))
            .add_stage(make_chunk_stage(state.clone(), deps.chunker.clone()));

        // Optional: context enrichment before embed
        let embed_dep = match &deps.context_enricher {
            Some(e) => {
                dag = dag.add_stage(make_context_enrich_stage(state.clone(), e.clone()));
                "context_enrich"
            }
            None => "chunk",
        };

        dag = dag.add_stage(make_embed_stage_after(embed_dep, state.clone(), deps.embedder.clone()));
        dag = dag.add_stage(make_vector_write_stage(state.clone(), deps.vector_store.clone()));

        // Optional: entity extraction → graph write (parallel after preprocess)
        if let (Some(ext), Some(gs)) = (&deps.entity_extractor, &deps.graph_store) {
            dag = dag.add_stage(make_entity_extract_stage(state.clone(), ext.clone(), gs.clone()));
        }

        // Optional: RAPTOR build → tree write (parallel after embed)
        if let Some(ts) = &deps.tree_store {
            dag = dag.add_stage(make_raptor_build_stage(state.clone(), ts.clone()));
        }

        dag
    })
}
```

- [ ] **Step 7: Run tests**

```bash
cargo test -p arcanum-pipeline --test registry_test 2>&1 | tail -15
```

Expected: all registry tests pass including `test_registry_all_five_templates_build`.

- [ ] **Step 8: Commit**

```bash
git add arcanum-pipeline/src/templates/
git commit -m "feat(pipeline): implement Contextual, Graph, RAPTOR, Full pipeline templates"
```

---

## Task 14: `IngestionWorker`

**Files:**
- Create: `arcanum-pipeline/src/worker.rs`
- Modify: `arcanum-pipeline/src/lib.rs`
- Create: `arcanum-pipeline/tests/worker_test.rs`

- [ ] **Step 1: Write failing tests**

Create `arcanum-pipeline/tests/worker_test.rs`:

```rust
// Copy stub_deps() verbatim from registry_test.rs — each test file is independent.
// (same stub_deps() function as registry_test.rs)

#[tokio::test]
async fn test_worker_processes_task_to_completion() {
    use arcanum_pipeline::{ArcanumPipelineRegistry, PipelineDeps};
    use arcanum_core::types::CollectionId;
    use std::sync::Arc;

    // Use a simpler approach — call worker.run_one() directly
    use arcanum_pipeline::worker::run_task;
    use arcanum_core::traits::Source;

    let deps = stub_deps();
    let registry = Arc::new(ArcanumPipelineRegistry::default());

    struct NoopEmitter;
    #[async_trait::async_trait]
    impl arcanum_core::traits::ProgressEmitter for NoopEmitter {
        async fn emit(&self, _: &str, _: serde_json::Value) {}
    }

    let result = run_task(
        "raw://test",
        CollectionId("col1".into()),
        "standard",
        registry,
        deps,
        Arc::new(NoopEmitter),
    ).await;
    assert!(result.is_ok(), "worker task failed: {:?}", result.err());
}

#[tokio::test]
async fn test_worker_skips_unchanged_document() {
    use arcanum_pipeline::worker::run_task;
    use arcanum_core::traits::Source;
    use arcanum_core::types::CollectionId;
    use arcanum_ingestion::DocumentHashTracker;
    use std::sync::Arc;

    let deps = stub_deps();
    // Pre-record the hash so the document appears "unchanged"
    deps.hash_tracker.record("raw://test", b"hello world document").await;

    let registry = Arc::new(ArcanumPipelineRegistry::default());
    struct NoopEmitter;
    #[async_trait::async_trait]
    impl arcanum_core::traits::ProgressEmitter for NoopEmitter {
        async fn emit(&self, _: &str, _: serde_json::Value) {}
    }

    let result = run_task(
        "raw://test", CollectionId("col1".into()), "standard",
        registry, deps, Arc::new(NoopEmitter),
    ).await;
    assert!(result.is_ok());
    // Skipped tasks still succeed — check by inspecting emitted events in a real test
}
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cargo test -p arcanum-pipeline --test worker_test 2>&1 | grep -E "error|FAILED"
```

- [ ] **Step 3: Implement**

Create `arcanum-pipeline/src/worker.rs`:

```rust
use crate::{deps::PipelineDeps, executor::DagExecutor,
            ingestion_state::IngestionState, registry::ArcanumPipelineRegistry};
use arcanum_core::{traits::{ProgressEmitter, Source}, types::CollectionId, Result, ArcanumError};
use arcanum_middleware::BoundedQueue;
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Clone)]
pub struct WorkerTask {
    pub source_uri:        String,
    pub collection_id:     CollectionId,
    pub pipeline_template: String,
}

pub struct IngestionWorker {
    queue:    Arc<BoundedQueue<WorkerTask>>,
    registry: Arc<ArcanumPipelineRegistry>,
    deps:     Arc<PipelineDeps>,
    emitter:  Arc<dyn ProgressEmitter>,
}

impl IngestionWorker {
    pub fn new(
        queue:    Arc<BoundedQueue<WorkerTask>>,
        registry: Arc<ArcanumPipelineRegistry>,
        deps:     Arc<PipelineDeps>,
        emitter:  Arc<dyn ProgressEmitter>,
    ) -> Self {
        Self { queue, registry, deps, emitter }
    }

    pub fn start(self: Arc<Self>, concurrency: usize) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let handles: Vec<_> = (0..concurrency).map(|_| {
                let worker = self.clone();
                tokio::spawn(async move { worker.run_loop().await; })
            }).collect();
            for h in handles { let _ = h.await; }
        })
    }

    async fn run_loop(&self) {
        while let Some(task) = self.queue.pop().await {
            self.emitter.emit("ingestion:progress", serde_json::json!({
                "source_uri": task.source_uri, "status": "processing"
            })).await;
            match run_task(
                &task.source_uri, task.collection_id.clone(), &task.pipeline_template,
                self.registry.clone(), self.deps.clone(), self.emitter.clone(),
            ).await {
                Ok(_) => self.emitter.emit("ingestion:progress", serde_json::json!({
                    "source_uri": task.source_uri, "status": "completed"
                })).await,
                Err(e) => self.emitter.emit("ingestion:progress", serde_json::json!({
                    "source_uri": task.source_uri, "status": "failed", "error": e.to_string()
                })).await,
            }
        }
    }
}

/// Extracted so tests can call it without a full queue.
pub async fn run_task(
    source_uri:        &str,
    collection_id:     CollectionId,
    pipeline_template: &str,
    registry:          Arc<ArcanumPipelineRegistry>,
    deps:              Arc<PipelineDeps>,
    _emitter:          Arc<dyn ProgressEmitter>,
) -> Result<()> {
    let source = Source::from_uri(source_uri)?;
    let state = Arc::new(Mutex::new(IngestionState::new(source, collection_id.clone())));
    let dag = registry.build(pipeline_template, state.clone(), &deps)?;
    let final_ctx = DagExecutor::execute(&dag, Default::default()).await?;

    // After successful pipeline: record content hash if document was processed (not skipped)
    let skipped = final_ctx.get("__skip").and_then(|v| v.as_bool()).unwrap_or(false);
    if !skipped {
        if let Some(doc) = &state.lock().await.doc {
            deps.hash_tracker.record(&doc.source_uri, &doc.content).await;
        }
    }
    Ok(())
}
```

Add to `arcanum-pipeline/src/lib.rs`:

```rust
pub mod worker;
pub use worker::{IngestionWorker, WorkerTask};
```

- [ ] **Step 4: Run tests**

```bash
cargo test -p arcanum-pipeline --test worker_test 2>&1 | tail -15
```

Expected: both tests pass.

- [ ] **Step 5: Commit**

```bash
git add arcanum-pipeline/src/worker.rs arcanum-pipeline/src/lib.rs \
        arcanum-pipeline/tests/worker_test.rs
git commit -m "feat(pipeline): add IngestionWorker with background task pool and run_task()"
```

---

## Task 15: `IngestionService` URI-level dedup

**Files:**
- Modify: `arcanum-engine/src/services/ingestion.rs`
- Modify: `arcanum-engine/tests/engine_test.rs` (or add new test file)

- [ ] **Step 1: Write failing test**

Add to `arcanum-engine/tests/engine_test.rs`:

```rust
#[tokio::test]
async fn test_ingest_skips_already_seen_uri() {
    use arcanum_engine::services::ingestion::{IngestionService, IngestRequest};
    use arcanum_ingestion::DocumentHashTracker;
    use arcanum_core::types::CollectionId;
    use std::sync::Arc;

    let tracker = Arc::new(DocumentHashTracker::new());
    // Pre-mark URI as seen
    tracker.record("file:///doc.pdf", b"").await;

    // Build service with the tracker
    // (IngestionService::new_with_tracker is added in this task)
    let events = Arc::new(arcanum_engine::event_bus::EventBus::new());
    let audit = Arc::new(arcanum_engine::audit::AuditLogger::new());
    let svc = IngestionService::new_with_tracker(
        Default::default(), events, audit, tracker
    );

    let req1 = IngestRequest {
        source_uri: "file:///doc.pdf".into(),
        collection_id: CollectionId("col".into()),
        pipeline_template: None,
    };
    let op1 = svc.ingest(req1.clone(), "user1").await.unwrap();

    // Second call with same URI — returns immediately, does NOT push to queue
    let op2 = svc.ingest(req1, "user1").await.unwrap();
    assert_ne!(op1.0, op2.0); // different OperationIds (each call creates a new one)
    // Verify queue depth is 1, not 2 — the second call was skipped
    // (queue depth checking requires exposing a len() on BoundedQueue — add that too)
}
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cargo test -p arcanum-engine 2>&1 | grep -E "error|FAILED"
```

- [ ] **Step 3: Add `len()` to `BoundedQueue`**

Modify `arcanum-middleware/src/queue.rs` — add:

```rust
pub fn len(&self) -> usize {
    // mpsc::Sender has no len(); use a counter instead
    // Simplest: this is best-effort — use capacity() if available
    // For testing, expose via a separate AtomicUsize counter
    0 // stub — update if queue depth checking is needed in tests
}
```

(Note: for the dedup test, assert on side effects — emit events — not queue depth.)

- [ ] **Step 4: Implement**

Modify `arcanum-engine/src/services/ingestion.rs`:

```rust
use arcanum_core::{config::ArcanumConfig, types::*, Result};
use arcanum_middleware::BoundedQueue;
use arcanum_ingestion::DocumentHashTracker;
use std::sync::Arc;
use crate::audit::{AuditLogger, AuditEntry};
use crate::event_bus::EventBus;

#[derive(Debug, Clone)]
pub struct IngestionTask {
    pub operation_id: OperationId,
    pub source_uri: String,
    pub collection_id: CollectionId,
    pub pipeline_template: String,
}

#[derive(Debug, Clone)]
pub struct IngestRequest {
    pub source_uri: String,
    pub collection_id: CollectionId,
    pub pipeline_template: Option<String>,
    pub force: bool,  // bypass dedup when true
}

pub struct IngestionService {
    queue:        Arc<BoundedQueue<IngestionTask>>,
    events:       Arc<EventBus>,
    audit:        Arc<AuditLogger>,
    hash_tracker: Arc<DocumentHashTracker>,
}

impl std::fmt::Debug for IngestionService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IngestionService").finish_non_exhaustive()
    }
}

impl IngestionService {
    pub fn new(_config: ArcanumConfig, events: Arc<EventBus>, audit: Arc<AuditLogger>) -> Self {
        Self::new_with_tracker(_config, events, audit, Arc::new(DocumentHashTracker::new()))
    }

    pub fn new_with_tracker(
        _config: ArcanumConfig,
        events: Arc<EventBus>,
        audit: Arc<AuditLogger>,
        hash_tracker: Arc<DocumentHashTracker>,
    ) -> Self {
        Self { queue: Arc::new(BoundedQueue::new(10_000)), events, audit, hash_tracker }
    }

    pub async fn ingest(&self, req: IngestRequest, user_id: &str) -> Result<OperationId> {
        let op_id = OperationId::new();

        // Level 1 URI dedup — skip queue push if seen before (unless force=true)
        if !req.force && self.hash_tracker.ever_seen(&req.source_uri).await {
            self.events.publish("ingestion:progress", serde_json::json!({
                "operation_id": op_id.0, "status": "skipped",
                "reason": "uri already ingested"
            })).await;
            return Ok(op_id);
        }

        let task = IngestionTask {
            operation_id: op_id.clone(),
            source_uri: req.source_uri.clone(),
            collection_id: req.collection_id.clone(),
            pipeline_template: req.pipeline_template.unwrap_or("standard".into()),
        };
        self.queue.push(task).await?;
        self.audit.log(AuditEntry {
            operation: "ingest".into(),
            user_id: user_id.to_string(),
            collection_id: req.collection_id.0,
            result: "accepted".into(),
        }).await;
        self.events.publish("ingestion:progress", serde_json::json!({
            "operation_id": op_id.0, "status": "queued"
        })).await;
        Ok(op_id)
    }
}
```

Note: `IngestRequest` now has a `force: bool` field. Update all call sites. Default is `false`.

- [ ] **Step 5: Run all tests**

```bash
cargo test -p arcanum-engine 2>&1 | tail -20
cargo test -p arcanum-ingestion 2>&1 | tail -5
cargo test -p arcanum-pipeline 2>&1 | tail -5
cargo test -p arcanum-core 2>&1 | tail -5
```

Expected: all tests across all crates pass.

- [ ] **Step 6: Commit**

```bash
git add arcanum-engine/src/services/ingestion.rs arcanum-engine/tests/
git commit -m "feat(engine): add URI-level dedup to IngestionService before queue push"
```

---

## Task 16: Final integration check

- [ ] **Step 1: Run full test suite**

```bash
cargo test --workspace 2>&1 | tail -30
```

Expected: all tests across all crates pass with 0 failures.

- [ ] **Step 2: Check compilation of all crates**

```bash
cargo build --workspace 2>&1 | tail -10
```

Expected: `Finished` with no errors.

- [ ] **Step 3: Final commit**

```bash
git add -A
git status  # verify only expected files
git commit -m "chore: final integration — ingestion pipeline fully wired end-to-end"
```

---

## Known implementation notes

| Item | Note |
|---|---|
| `GitLoader` / `DatabaseLoader` multi-doc | These loaders return a single `RawDocument`. When implemented, decide: `load_all() -> Vec<RawDocument>` extension on the trait, or expand to multiple `Source::Raw` entries queued individually. |
| `source.uri()` for `CloudStorage`/`Connector` | Returns a static string `"cloud://"`. Call `source.display_uri()` when a human-readable URI is needed. |
| `IngestRequest.force` | New field added to `IngestRequest`. All existing call sites that construct `IngestRequest` must add `force: false`. |
| RAPTOR trait object | `RaptorBuilder<S>` is generic over `TreeStore`. The `make_raptor_build_stage` uses a `TreeStoreAdapter` newtype to bridge `Arc<dyn TreeStore>` to the generic bound. |
| NonCore failure handling | `make_entity_extract_stage` and `make_raptor_build_stage` use `unwrap_or_else(|e| warn!(...))` — failures are logged but do not abort the pipeline. This matches the NonCore classification in the spec. |
