# Building an Enterprise RAG System with Arcanum

A practical guide to assembling a production-grade RAG system from Arcanum's primitives. It walks through every decision point — backend selection, chunking configuration, retrieval strategy, experiment workflow, and operations — in the order you encounter them when building a real system.

---

## 1. Workspace Setup

Add Arcanum crates to your `Cargo.toml`. You will always need the core crates; pull in optional ones based on which backends you wire:

```toml
[dependencies]
# Required
arcanum-core    = { path = "../arcanum/arcanum-core" }
arcanum-engine  = { path = "../arcanum/arcanum-engine" }

# Choose your storage adapters
arcanum-vector  = { path = "../arcanum/arcanum-vector" }   # LanceDB, PgVector, Tantivy
arcanum-graph   = { path = "../arcanum/arcanum-graph" }    # Neo4j, in-memory graph
arcanum-tree    = { path = "../arcanum/arcanum-tree" }     # RAPTOR tree

# Model providers
arcanum-models  = { path = "../arcanum/arcanum-models" }   # Ollama, OpenAI embedders

# Optional tooling
arcanum-chunk-eval = { path = "../arcanum/arcanum-chunk-eval" } # offline benchmarks
arcanum-eval       = { path = "../arcanum/arcanum-eval" }       # retrieval quality metrics

tokio = { version = "1", features = ["full"] }
```

Everything compiles to a single binary. The `ArcanumEngine` builder validates wiring at startup — missing dependencies produce clear errors naming exactly what is needed, not panics at query time.

---

## 2. Configuration

Arcanum reads configuration in layers: compile-time defaults → `config.toml` → environment variables (highest priority). Environment variables are prefixed `ARCANUM_` and use double-underscores for nesting (`ARCANUM_STORAGE__METADATA_BACKEND=postgres`).

Start with a `config.toml` at your project root:

```toml
[global]
runtime_mode = "production"   # development | production | enterprise

[ingestion]
worker_pool_size    = 16
queue_capacity      = 50000
retry_max_attempts  = 5
retry_base_delay_ms = 500

# Optional — omit to use the built-in parsers (PDF, HTML, EPUB, DOCX)
[ingestion.docling.backend]
type             = "http"
base_url         = "http://docling-serve:5001"
timeout_secs     = 300
use_async        = false

[ingestion.chunking.vector]
strategy = "semantic"
params   = { max_chars = 800 }

[ingestion.chunking.graph]
strategy = "hierarchical"
params   = {}

[ingestion.chunking.tree]
strategy = "fixed"
params   = { chunk_size = 1024, overlap = 128 }

[embedding]
provider   = "ollama"
model_id   = "nomic-embed-text"
dimension  = 768
batch_size = 32

[retrieval]
top_k              = 10
orchestration_mode = "ParallelFusion"
fusion_strategy    = "Rrf"

[storage]
metadata_backend = "postgres"
vector_backend   = "lancedb"
graph_enabled    = true
tree_enabled     = true

[admin]
portal_enabled       = true
audit_retention_days = 90

[server]
cors_allowed_origins = ["https://app.internal.example.com"]
```

### Runtime Modes

`development` allows SQLite for metadata and accepts any configuration. Use it for local iteration.

`production` requires Postgres as the metadata backend. SQLite is rejected at startup.

`enterprise` adds enforcement on top of `production`: admin JWT (`RS256`) validation, IP allowlist, audit retention minimum. Use this for regulated workloads.

Switch modes via environment:

```bash
ARCANUM_GLOBAL__RUNTIME_MODE=enterprise cargo run
```

---

## 3. Choosing Backends

Every backend is a trait implementation. You swap them without touching any pipeline or retrieval code — just the builder wiring.

### Vector Store

For most teams, start with **LanceDB** (zero-infrastructure, local or S3-backed) and migrate to PgVector when you already have Postgres and want one fewer infrastructure component.

```rust
use arcanum_vector::lance::LanceVectorStore;
use arcanum_vector::pgvector::PgVectorStore;

// LanceDB — local or S3
let vector_store = LanceVectorStore::open("/data/vectors").await?;
// or s3://bucket/vectors with AWS credentials in environment

// PgVector — for existing Postgres shops
let vector_store = PgVectorStore::connect(&database_url, dimension).await?;
```

Tantivy BM25 is included automatically when you use LanceDB — no separate wiring needed for lexical search.

### Graph Store

Use the **in-memory graph** for development and under 100k entities. Use **Neo4j** for production graph workloads.

```rust
use arcanum_graph::memory::InMemoryGraphStore;
use arcanum_graph::neo4j::Neo4jStore;

// Development
let graph_store = InMemoryGraphStore::new();

// Production
let graph_store = Neo4jStore::connect("bolt://neo4j:7687", "neo4j", &password).await?;
```

If you omit `graph_store` from the builder entirely, graph and entity-extraction stages are silently skipped. No config changes needed — omitting the dependency is the disable switch.

### Tree Store

The RAPTOR tree needs its own storage for cluster centroids and tree structure. Use the **in-memory store** for development, **Postgres** for production.

```rust
use arcanum_tree::memory::InMemoryTreeStore;
use arcanum_tree::postgres::PgTreeStore;

let tree_store = PgTreeStore::connect(&database_url).await?;
```

### Embedder

Arcanum ships adapters for Ollama (local) and OpenAI-compatible APIs. Both implement `Embedder` identically from the engine's perspective.

```rust
use arcanum_models::ollama::OllamaEmbedder;
use arcanum_models::openai::OpenAIEmbedder;

// Local Ollama
let embedder = OllamaEmbedder::new("http://localhost:11434", "nomic-embed-text", 768);

// OpenAI-compatible (also works for Azure, Together AI, etc.)
let embedder = OpenAIEmbedder::new(
    "https://api.openai.com/v1",
    &api_key,
    "text-embedding-3-small",
    1536,
);
```

---

## 4. Wiring the Engine

The `ArcanumEngine::builder()` validates that all provided dependencies are consistent and returns a running engine.

```rust
use arcanum_engine::ArcanumEngine;
use arcanum_core::config::ArcanumConfig;
use std::sync::Arc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = ArcanumConfig::from_file("config.toml")?;

    let engine = ArcanumEngine::builder()
        .config(config)
        .auth_secret(std::env::var("ARCANUM_AUTH_SECRET")?)
        .vector_store(Arc::new(lance_store))
        .embedder(Arc::new(ollama_embedder))
        .enricher(Arc::new(llm_enricher))       // enables contextual + graph entity extraction
        .graph_store(Arc::new(neo4j_store))     // enables graph ingestion + retrieval
        .tree_store(Arc::new(pg_tree_store))    // enables RAPTOR ingestion + retrieval
        .secret_store(Arc::new(vault_store))    // enables hot-reload from Vault
        .build()
        .await?;

    // engine.ingestion   — IngestionService
    // engine.retrieval   — RetrievalService
    // engine.experiment  — ExperimentService
    // engine.collection  — CollectionService
    // engine.auth        — AuthService

    Ok(())
}
```

`build()` performs these steps in order:
1. Validates `runtime_mode` against wired backends.
2. Instantiates `ChunkRegistry` with config defaults.
3. Builds `PerBackendChunkers` from `ingestion.chunking`.
4. Creates the document registry and ingestion worker pool.
5. Wires retrievers based on which stores are present.
6. Starts background tasks: secret reload loop, experiment eval loop.

Any misconfiguration (bad strategy name, dimension mismatch, missing Postgres connection string in `production` mode) is returned from `build()` as an error — not discovered later.

---

## 5. Managing Collections

A collection is the unit of isolation. Every document, chunk, and experiment is scoped to a collection. Collections carry their own chunker config that overrides the global default.

### Create a collection

```rust
use arcanum_core::types::{CollectionId, PerBackendChunkConfig, ChunkStrategyConfig};

engine.collection.create(CreateCollectionRequest {
    id:          CollectionId("legal-contracts".into()),
    description: Some("Legal contract corpus".into()),
    chunker_config: Some(PerBackendChunkConfig {
        vector: Some(ChunkStrategyConfig {
            strategy: "semantic".into(),
            params:   serde_json::json!({ "max_chars": 600 }),
        }),
        graph:  Some(ChunkStrategyConfig {
            strategy: "hierarchical".into(),
            params:   serde_json::json!({}),
        }),
        tree:   None,  // falls back to global default
    }),
}).await?;
```

### Via HTTP

```bash
# Create
curl -X POST /api/v1/collections \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "id": "legal-contracts",
    "description": "Legal contract corpus",
    "chunker_config": {
      "vector": { "strategy": "semantic", "params": { "max_chars": 600 } },
      "graph":  { "strategy": "hierarchical", "params": {} }
    }
  }'

# List
curl -H "Authorization: Bearer $TOKEN" /api/v1/collections

# Delete
curl -X DELETE -H "Authorization: Bearer $TOKEN" /api/v1/collections/legal-contracts
```

---

## 6. Ingestion Pipelines

Ingestion is async and queue-backed. `ingest()` enqueues a task and returns an `operation_id` for tracking. A worker pool processes tasks from the queue, respecting retry policy.

### Choosing a pipeline template

| Template | When to use |
|---|---|
| `standard` | Pure vector search. Fastest, least infrastructure. |
| `contextual` | When chunks need document-level context prefix for better precision. |
| `graph` | When relationship-heavy queries are important (who-knows-who, part-of). |
| `raptor` | When questions need abstractive reasoning across multiple documents. |
| `full` | Maximum retrieval coverage — run all strategies. |

For a new enterprise RAG system, start with `standard`. Add `graph` when entity-relationship queries become important, add `raptor` when abstractive summarisation queries start failing.

### Ingest a document

```rust
use arcanum_core::types::{IngestRequest, CollectionId};

let op_id = engine.ingestion.ingest(IngestRequest {
    source_uri:        "s3://internal-docs/q4-contract.pdf".into(),
    collection_id:     CollectionId("legal-contracts".into()),
    pipeline_template: Some("full".into()),
    force:             false,  // true to re-ingest unchanged documents
    content:           None,   // or Some(bytes) to push content directly
    mime_hint:         None,   // or Some("application/pdf")
}, &user_id).await?;
```

When `content` is `None`, the loader resolves `source_uri` using the configured loaders (S3, HTTP, local filesystem). When `content` is `Some`, it bypasses the loader and uses the provided bytes with `mime_hint` for format selection.

`force: false` is the default — if the document's content hash matches a previous ingest, the pipeline skips it. Set `force: true` to re-chunk and re-embed even unchanged documents (useful after a chunker config change).

### Bulk ingest

There is no separate batch API — submit one `ingest()` call per document. The worker pool handles concurrency. For large corpora, submit all tasks immediately and track them in parallel:

```rust
let operation_ids: Vec<_> = source_uris
    .iter()
    .map(|uri| engine.ingestion.ingest(IngestRequest {
        source_uri:        uri.clone(),
        collection_id:     CollectionId("legal-contracts".into()),
        pipeline_template: Some("full".into()),
        force:             false,
        content:           None,
        mime_hint:         None,
    }, &user_id))
    .collect::<FuturesUnordered<_>>()
    .try_collect()
    .await?;
```

### Direct file upload (HTTP)

```bash
curl -X POST "/api/v1/upload?collection_id=legal-contracts&filename=contract.pdf&pipeline=full" \
  -H "Authorization: Bearer $TOKEN" \
  --data-binary @/path/to/contract.pdf
```

### Document deduplication behaviour

The document registry tracks `(source_uri, collection_id)` pairs. On ingest:

- **Unchanged** (`hash matches`) → skipped entirely. No worker slots consumed.
- **Changed** (`hash differs`) → previous chunks are deleted, document is re-ingested.
- **New** → normal ingestion flow.
- **Interrupted cleanup** (previous run deleted the document record but left chunks) → registry detects `Replacing` status on the next ingest and runs cleanup before ingesting.

Deduplication happens before any expensive model calls — a full corpus re-crawl touching unchanged documents is cheap.

---

## 7. Choosing a Preprocessor Backend

Before chunking, each document passes through a preprocessor chain that converts raw bytes to clean text. Arcanum ships two preprocessor sets:

| Set | Covered formats | Dependency |
|---|---|---|
| Built-in (`default_chains`) | PDF, HTML, XHTML, EPUB, DOCX | None — bundled parsers |
| `DoclingPreprocessor` | PDF, DOCX, PPTX, XLSX, EPUB, HTML, XHTML, PNG, JPEG, TIFF | docling-serve or `docling` CLI |

Use the built-in set for the five common formats with no added infrastructure. Switch to `DoclingPreprocessor` when you need to handle presentations, spreadsheets, or images.

The engine reads `[ingestion.docling]` at startup. If the section is present, it builds a `DoclingPreprocessor` and wires it to all 10 MIME types via `PreprocessorRegistry::docling_chains()`. If the section is absent, `default_chains()` is used.

### HTTP backend (docling-serve)

Run a [docling-serve](https://github.com/DS4SD/docling-serve) instance and point Arcanum at it:

```toml
[ingestion.docling.backend]
type             = "http"
base_url         = "http://docling-serve:5001"
api_key          = "optional-bearer-token"
timeout_secs     = 300    # total budget for upload + polling; default 300
use_async        = true   # false = blocking POST; true = submit then poll
poll_interval_ms = 2000   # polling cadence when use_async = true; default 2000
```

Both `use_async = false` (synchronous) and `use_async = true` (poll-based) share the same `timeout_secs` budget. Choose `use_async = true` for large documents or batch workloads where a synchronous POST would time out upstream.

### CLI backend

Shell out to a local `docling` binary. Useful for air-gapped environments or local development without a server process:

```toml
[ingestion.docling.backend]
type    = "cli"
command = "docling"   # path or name on $PATH
```

The CLI backend writes the document to a temporary file, runs the command, and reads Markdown output from stdout.

### Format gap in built-in preprocessing

If you stay with the built-in preprocessors, note that PPTX, XLSX, PNG, JPEG, and TIFF files pass through as raw bytes — no text is extracted. These documents will produce empty or near-empty chunks. Enable `DoclingPreprocessor` when your corpus includes these formats.

---

## 8. Configuring Chunking per Collection

Two-tier precedence:

1. **Global default** (`config.toml [ingestion.chunking]`) — applies when a collection has no override.
2. **Per-collection override** (`CollectionInfo.chunker_config`) — takes precedence over the global default.

Resolution happens at job-start time. A bad config (unknown strategy, invalid params) fails the job immediately with a descriptive error — not mid-chunk.

### Selecting chunking strategies

The right strategy depends on your content type and downstream use:

| Content type | Vector | Graph | Tree |
|---|---|---|---|
| Legal / compliance documents | `semantic` (600–900 chars) | `hierarchical` | `fixed` (1024 tokens) |
| Scientific papers | `semantic` (800–1200 chars) | `hierarchical` | `semantic` |
| Support knowledge base | `fixed` (512 tokens) | — | `fixed` |
| Markdown / technical docs | `structure_aware` | `hierarchical` | `fixed` |
| Short product descriptions | `propositional` | — | — |

**`fixed`** — deterministic token counts. Predictable embedding cost. Best when content is uniform.

**`semantic`** — splits at sentence boundaries, keeping semantically coherent chunks. Higher quality embeddings for prose.

**`propositional`** — splits into atomic claims. Very small chunks, very high precision for fact-retrieval. High chunk count increases embedding cost.

**`hierarchical`** — preserves heading/section structure. Best for knowledge graph extraction where structural relationships matter.

**`structure_aware`** — understands Markdown / HTML. Respects headings, code blocks, and lists. Best for documentation corpora.

### Update a collection's chunker config

```bash
curl -X PATCH /api/v1/collections/legal-contracts \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "chunker_config": {
      "vector": { "strategy": "semantic", "params": { "max_chars": 700 } }
    }
  }'
```

Note: updating the config only affects new ingestion jobs. Re-ingest existing documents with `force: true` to apply the new strategy to existing content.

---

## 9. Retrieval

Retrieval is request-scoped and stateless. The `RetrievalService` selects a retrieval strategy based on the orchestration mode configured in `config.toml`.

```rust
use arcanum_core::types::{Query, CollectionId};

let results = engine.retrieval.search(
    Query::new("payment obligations under force majeure")
        .with_collection(CollectionId("legal-contracts".into()))
        .with_top_k(10),
    &claims,
).await?;

for result in results {
    println!("[{:.3}] ({}) {}", result.score, result.document_id, result.chunk_text);
}
```

### Orchestration modes

**`Static`** — a fixed retriever is used for every query. Configure in `config.toml`:

```toml
[retrieval]
orchestration_mode = "Static"
```

**`QueryClassified`** — the query is sent to a lightweight classifier that routes it to the most appropriate retriever. Keyword-heavy queries go to BM25; entity-relationship queries go to Graph; abstract questions go to RAPTOR. Reduces unnecessary work for simple queries.

```toml
[retrieval]
orchestration_mode = "QueryClassified"
```

**`ParallelFusion`** (recommended for production) — all configured retrievers run concurrently and results are merged using Reciprocal Rank Fusion (RRF), keyed on `document_id`. A document appearing in both vector and graph results is boosted; ordering within each retriever is preserved as a tie-breaker.

```toml
[retrieval]
orchestration_mode = "ParallelFusion"
fusion_strategy    = "Rrf"
```

### HTTP search

```bash
curl -X POST /api/v1/search \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "query": "payment obligations under force majeure",
    "collection_id": "legal-contracts",
    "top_k": 10
  }'
```

---

## 10. Optimising Chunking with Shadow Experiments

Shadow experiments let you test a challenger chunking strategy on live ingestion traffic without affecting queries. New documents are written to both the live collection and a shadow namespace. An automated eval loop computes recall metrics for both sides. When the challenger shows a statistically meaningful win, it becomes `ReadyToPromote`.

### Workflow

**Step 1: Measure the baseline.** Before starting an experiment, run the offline benchmark harness to establish a recall baseline for the current strategy.

```bash
curl -X POST /api/v1/chunk/benchmark \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "corpus": [
      { "source_uri": "doc1", "content": "The transformer architecture..." },
      { "source_uri": "doc2", "content": "Force majeure is a clause..." }
    ],
    "queries": [
      { "text": "transformer architecture", "expected_doc_ids": ["doc1"] },
      { "text": "force majeure clause", "expected_doc_ids": ["doc2"] }
    ],
    "strategies": [
      { "vector": { "strategy": "fixed",    "params": { "chunk_size": 512, "overlap": 64 } } },
      { "vector": { "strategy": "semantic", "params": { "max_chars": 800 } } }
    ]
  }'
```

The response includes `recall_at_5`, `recall_at_10`, `mean_chunk_tokens`, and `chunk_size_p50`/`p95` per strategy. Pick the best candidate as the challenger.

**Step 2: Explore with the inspect API.** Before committing to a full benchmark, use the stateless inspect endpoint to compare how strategies split a sample document. No storage, no embeddings — instant feedback.

```bash
curl -X POST /api/v1/chunk/inspect \
  -H "Content-Type: application/json" \
  -d '{
    "text": "Article 12: Payment Obligations. Each party shall...",
    "strategies": [
      { "strategy": "fixed",    "params": { "chunk_size": 512, "overlap": 64 } },
      { "strategy": "semantic", "params": { "max_chars": 800 } }
    ]
  }'
```

**Step 3: Start the shadow experiment.**

```bash
curl -X POST /api/v1/collections/legal-contracts/experiments \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "vector": { "strategy": "semantic", "params": { "max_chars": 700 } },
    "graph":  null,
    "tree":   null
  }'
# → { "experiment_id": "01924f2a-...", "status": "Active", ... }
```

Only the `vector` field is overriding here; `graph` and `tree` remain at collection or global defaults. At most one active experiment per collection at a time — attempting to start a second returns HTTP 409.

**Step 4: Monitor.**

```bash
curl /api/v1/collections/legal-contracts/experiments/01924f2a-... \
  -H "Authorization: Bearer $TOKEN"
# → { "status": "ReadyToPromote", "metrics": { "challenger_recall_at_5": 0.87, ... } }
```

The experiment transitions to `ReadyToPromote` when the background eval loop determines the challenger is ahead by the configured threshold over a sufficient document sample.

**Step 5: Promote or abandon.**

```bash
# Promote — collection's chunker_config updated; new ingestion uses promoted strategy
curl -X POST /api/v1/collections/legal-contracts/experiments/01924f2a-.../promote \
  -H "Authorization: Bearer $TOKEN"

# Abandon — collection's chunker_config unchanged
curl -X DELETE /api/v1/collections/legal-contracts/experiments/01924f2a-... \
  -H "Authorization: Bearer $TOKEN"
```

Shadow writes are best-effort — a failure to write to the shadow namespace never fails the primary ingestion job. The primary collection is always the source of truth for queries.

---

## 11. Retrieval Quality Evaluation

`arcanum-eval` provides scheduled, automated measurement of retrieval quality against golden datasets.

```rust
use arcanum_eval::{BenchmarkDataset, EvalConfig};

// Register a golden dataset
engine.eval.register_dataset(BenchmarkDataset {
    name:    "legal-golden".into(),
    queries: vec![
        EvalQuery {
            text:               "force majeure payment obligations".into(),
            expected_doc_ids:   vec!["doc-42".into(), "doc-87".into()],
            collection_id:      CollectionId("legal-contracts".into()),
        },
    ],
}).await?;

// Run on-demand — or schedule with eval.schedule_cron in config.toml
let report = engine.eval.run("legal-golden").await?;
println!("MRR: {:.3}  Hit@5: {:.3}  NDCG@10: {:.3}",
    report.mrr, report.hit_rate_at_5, report.ndcg_at_10);
```

Via MCP (for AI assistants):

```json
{
  "tool": "eval_run",
  "arguments": { "collection_id": "legal-contracts" }
}
```

---

## 12. Authentication

Generate API keys via the admin API:

```bash
# Create a scoped key for a user
curl -X POST /admin/api-keys \
  -H "Authorization: Bearer $ADMIN_JWT" \
  -H "Content-Type: application/json" \
  -d '{
    "user_id":             "user:alice",
    "allowed_collections": ["legal-contracts", "hr-policies"],
    "is_admin":            false
  }'
# → { "token": "arc_..." }
```

Tokens are `HS256` JWTs signed with `ARCANUM_AUTH_SECRET`. The token encodes `user_id`, `allowed_collections`, and `is_admin`. Every request validates the token and checks collection scope before executing.

Admin operations require `is_admin: true`. In `enterprise` mode, admin routes additionally require an `RS256` JWT signed with the configured admin public key.

### RBAC roles

Assign a role at key creation to restrict what an admin user can do:

| Role | Can |
|---|---|
| `Tester` | Read health, metrics |
| `Operator` | Manage collections, audit log, experiments, ingestion |
| `Admin` | Everything, including key rotation and destructive operations |

---

## 13. Running the HTTP Server

The `arcanum-server` crate exposes the REST API, admin portal, and WebSocket event bus.

```rust
use arcanum_server::Server;

Server::new(engine)
    .bind("0.0.0.0:8080")
    .admin_bind("127.0.0.1:9090")    // admin portal on separate port
    .start()
    .await?;
```

Or as a standalone binary:

```bash
ARCANUM_AUTH_SECRET=your-secret \
ARCANUM_STORAGE__METADATA_BACKEND=postgres \
DATABASE_URL=postgres://user:pass@db:5432/arcanum \
cargo run -p arcanum-server
```

### Key API routes

| Method | Path | Description |
|---|---|---|
| `POST` | `/api/v1/search` | Semantic search |
| `POST` | `/api/v1/ingest` | Enqueue a document for ingestion |
| `POST` | `/api/v1/upload` | Direct file upload |
| `GET` | `/api/v1/collections` | List collections |
| `POST` | `/api/v1/collections` | Create a collection |
| `PATCH` | `/api/v1/collections/:id` | Update collection config |
| `DELETE` | `/api/v1/collections/:id` | Delete a collection |
| `POST` | `/api/v1/collections/:id/experiments` | Start shadow experiment |
| `GET` | `/api/v1/collections/:id/experiments/:exp_id` | Get experiment status |
| `POST` | `/api/v1/collections/:id/experiments/:exp_id/promote` | Promote challenger |
| `DELETE` | `/api/v1/collections/:id/experiments/:exp_id` | Abandon experiment |
| `POST` | `/api/v1/chunk/inspect` | Stateless strategy comparison |
| `POST` | `/api/v1/chunk/benchmark` | Offline recall benchmark |
| `GET` | `/evidence/chunk/:chunk_id` | Trace a chunk back to its source document |
| `GET` | `/evidence/tree-node/:node_id` | Trace a RAPTOR tree node to its source chunks |
| `GET` | `/evidence/entity/:entity_id` | Trace a graph entity to its source chunks |
| `GET` | `/evidence/relation/:source_id/:relation_type/:target_id` | Trace a graph relation to its source chunks |
| `GET` | `/ws/events` | WebSocket event subscription |
| `POST` | `/admin/api-keys` | Issue API key |
| `POST` | `/admin/rotate-keys` | Trigger secret reload |
| `POST` | `/admin/gc` | Run retention-based garbage collection once |

---

## 14. Evidence & Document Provenance

Every chunk, RAPTOR tree summary, graph entity, and graph relation can be traced back to the document version and byte range it came from. This is what answers "show me the source" for a retrieved result — citations, compliance review, debugging a bad chunk.

### How it fits together

```
Ingest ─→ snapshot (raw bytes + canonical JSON) ─→ chunk ─→ embed ─→ vector_write
                │                                                        │
                ▼                                                        ▼
        DocumentVersionStore                                    ChunkMetadataStore
   (document_id, version_num,                              (chunk_id → document_id,
    status, content_hash)                                   version_num, source_uri,
                                                              snapshot_uri, page/section,
                                                              offset_start/offset_end)
```

`make_vector_write_stage` writes a `ChunkMetadataRecord` for every chunk right after the vector store upsert succeeds — never before, so a failed vector write can't leave orphaned metadata. The record carries the chunk's exact byte range in the source document (`ChunkPosition::start`/`end`), not just which document it came from.

### Wiring it up

Four new builder methods, all optional:

```rust
use arcanum_ingestion::{SqliteDocumentVersionStore, LocalSnapshotStore};
use arcanum_core::traits::InMemoryChunkMetadataStore;
use arcanum_evidence::DefaultEvidenceResolver;
use std::sync::Arc;

let version_store        = Arc::new(SqliteDocumentVersionStore::open("data/versions.db").await?);
let snapshot_store        = Arc::new(LocalSnapshotStore::new("data/snapshots"));
let chunk_metadata_store  = Arc::new(InMemoryChunkMetadataStore::new());

let evidence_resolver = Arc::new(DefaultEvidenceResolver::new(
    chunk_metadata_store.clone(),
    version_store.clone(),
    tree_store.clone(),   // reuses the same tree_store wired for RAPTOR
    graph_store.clone(),  // reuses the same graph_store wired for graph retrieval
));

let engine = ArcanumEngine::builder()
    // ...
    .version_store(version_store)
    .snapshot_store(snapshot_store)
    .chunk_metadata_store(chunk_metadata_store)
    .evidence(evidence_resolver)
    .build()
    .await?;
```

Production swaps: `SqliteDocumentVersionStore` → `PostgresDocumentVersionStore`, `InMemoryChunkMetadataStore` → `PostgresChunkMetadataStore`, `LocalSnapshotStore` → an S3-backed `SnapshotStore` impl. `DefaultEvidenceResolver` itself doesn't change — it only depends on the four trait objects, not their concrete backends.

If you skip `.evidence(...)`, the `/evidence/*` routes return `503` rather than `404` or panicking — the rest of the engine is unaffected.

### Versioning policy

Set per collection via `DocumentVersionStore::set_versioning_policy`:

| Policy | Behaviour | When to use |
|---|---|---|
| `Replace` (default) | Re-ingest supersedes the prior version immediately; only the latest is queryable | Most collections — content correction, no audit need |
| `AppendOnly` | Every version stays `Active` forever | Regulatory record-keeping where nothing is ever deleted |
| `RetentionBased { days }` | Superseded versions are kept for `days`, then GC'd | Audit trail with a bounded retention window |

```rust
engine.version_store.set_versioning_policy(
    "legal-contracts",
    VersioningPolicy::RetentionBased { days: 90 },
).await?;
```

### Resolving evidence

```rust
let chain = engine.evidence.as_ref().unwrap()
    .resolve_chunk(&chunk_id)
    .await?;

// chain.root         — ProofNode { kind: Chunk, label: "confluence://page/42 p.3 §3.2", .. }
// chain.raw_sources  — Vec<RawSourceRef>, one per cited document span:
//   document_id, version_num, source_uri, snapshot_uri, page, section,
//   block_ids, offset_start, offset_end
```

`resolve_tree_node` / `resolve_entity` / `resolve_relation` work the same way but fan out: the returned `ProofNode.children` lists every chunk that contributed, and `raw_sources` is deduplicated by `(snapshot_uri, offset_start, offset_end)` so two children citing the exact same span don't produce duplicate citations — distinct spans from the same document are kept.

`resolve_chunk` also cross-checks that the cited version is still `Active`; if it's been superseded or deleted (e.g. by GC), `chain.root.metadata.version_status` reports `"superseded"`, `"deleted"`, or `"unknown"` instead of silently returning stale evidence as if it were current.

Via HTTP:

```bash
curl -H "Authorization: Bearer $TOKEN" /evidence/chunk/3f29...
curl -H "Authorization: Bearer $TOKEN" /evidence/tree-node/8a01...
curl -H "Authorization: Bearer $TOKEN" /evidence/entity/c402...
curl -H "Authorization: Bearer $TOKEN" /evidence/relation/c402.../REPORTS_TO/9f11...
```

Each returns `200` with a `ProofChain`, `404` if the ID doesn't resolve, or `503` if no resolver is configured.

### Garbage collection

For `RetentionBased` collections, `GcWorker` removes the snapshot, vector chunks, tree nodes, graph entities, and chunk metadata for each superseded version once it ages past the retention window — scoped to that exact version, so an active version sharing the same `source_uri` is never touched.

```rust
use arcanum_ingestion::PostgresGcWorker;

let gc_worker = Arc::new(PostgresGcWorker::new(
    &db_url, version_store.clone(), snapshot_store.clone(), vector_store.clone(),
    tree_store.clone(), graph_store.clone(), chunk_metadata_store.clone(),
).await?);

let engine = ArcanumEngine::builder()
    // ...
    .gc_worker(gc_worker)
    .build()
    .await?;
```

`PostgresGcWorker` requires Postgres — its bookkeeping query joins `document_versions` and `source_documents`, so there's no in-memory equivalent for local development. Trigger a pass on a schedule (cron, k8s CronJob):

```bash
curl -X POST /admin/gc -H "Authorization: Bearer $ADMIN_TOKEN"
# → { "versions_deleted": 12, "snapshots_removed": 12, "chunks_removed": 340, "errors": [] }
```

A single version's deletion failure (e.g. a transient store error) is recorded in `errors` and that version stays `superseded` for retry on the next pass — the rest of the batch still completes.

---

## 15. MCP Integration

Mount the MCP server alongside your HTTP server to expose search and ingestion to AI assistants:

```rust
use arcanum_mcp::McpServer;

McpServer::new(engine.clone())
    .bind("0.0.0.0:3000")
    .start()
    .await?;
```

Configure Claude to use it in `.claude/config.json`:

```json
{
  "mcpServers": {
    "arcanum": {
      "command": "nc",
      "args": ["localhost", "3000"]
    }
  }
}
```

Claude can then call `search`, `ingest`, `list_collections`, and `eval_run` directly in its tool-use loop. Each call requires a valid Bearer token passed as an `Authorization` header — no shared session, no bypass.

---

## 16. Observability

`arcanum-telemetry` exposes structured traces and Prometheus-compatible metrics.

### Tracing

Arcanum emits OpenTelemetry spans for every ingestion job, retrieval request, and experiment lifecycle event. Configure an OTLP exporter:

```toml
[telemetry]
otlp_endpoint = "http://otel-collector:4317"
service_name  = "arcanum"
```

Or to stdout in development:

```toml
[telemetry]
exporter = "stdout"
log_level = "debug"
```

### Metrics

Expose the Prometheus scrape endpoint:

```toml
[telemetry]
metrics_bind = "0.0.0.0:9091"
```

Key metrics to alert on:

| Metric | Alert condition |
|---|---|
| `arcanum_request_duration_seconds{p99}` | > 2 s for search |
| `arcanum_ingest_docs_total{status="error"}` | Rate > 0 sustained |
| `arcanum_circuit_breaker_state{backend="embedder"}` | == "open" |
| `arcanum_circuit_breaker_state{backend="vector_store"}` | == "open" |

The Grafana dashboard stack ships in `arcanum-telemetry/grafana/`. Import the JSON dashboard files into your Grafana instance — they assume a Prometheus datasource named `arcanum-metrics`.

---

## 17. Production Deployment

### Docker

```dockerfile
FROM rust:1.78-slim AS builder
WORKDIR /app
COPY . .
RUN cargo build --release -p arcanum-server

FROM debian:bookworm-slim
COPY --from=builder /app/target/release/arcanum-server /usr/local/bin/
EXPOSE 8080 9090 9091
CMD ["arcanum-server"]
```

### Environment variables for production

```bash
# Required
ARCANUM_GLOBAL__RUNTIME_MODE=production
ARCANUM_AUTH_SECRET=<32+ char random secret>
DATABASE_URL=postgres://user:pass@db:5432/arcanum

# Storage
ARCANUM_STORAGE__VECTOR_BACKEND=lancedb
ARCANUM_VECTOR__LANCEDB__PATH=s3://your-bucket/vectors
AWS_ACCESS_KEY_ID=...
AWS_SECRET_ACCESS_KEY=...

# Embedding
ARCANUM_EMBEDDING__PROVIDER=ollama
ARCANUM_EMBEDDING__MODEL_ID=nomic-embed-text
ARCANUM_EMBEDDING__BASE_URL=http://ollama:11434

# Graph (optional)
NEO4J_URI=bolt://neo4j:7687
NEO4J_PASSWORD=...

# Telemetry
ARCANUM_TELEMETRY__OTLP_ENDPOINT=http://otel-collector:4317
```

### Secret hot-reload with Vault

```rust
use arcanum_models::vault::VaultSecretStore;

let vault = VaultSecretStore::new(
    "https://vault.internal:8200",
    "secret/data/arcanum",
    &vault_token,
    300,  // reload interval seconds
)?;

let engine = ArcanumEngine::builder()
    // ...
    .secret_store(Arc::new(vault))
    .build()
    .await?;
```

`POST /admin/rotate-keys` triggers an immediate reload outside the normal interval — useful during incident response.

### Health checks

```bash
# Liveness — process is alive
GET /health

# Readiness — all backends reachable
GET /health/ready
```

Use `/health/ready` as the Kubernetes readiness probe. It checks the vector store, metadata backend, and embedder circuit breaker state. A failing component returns HTTP 503 with the component name in the response body.

---

## 18. Common Pitfalls

**Changing chunker config without re-ingesting.** Updating a collection's `chunker_config` affects only future ingestion jobs. Existing chunks were produced by the old strategy and remain in the store. To apply the new strategy to existing documents, ingest them again with `force: true`. Shadow experiments exist precisely to validate a new strategy before committing to a full re-ingest.

**Setting `force: true` on large corpora.** This bypasses the deduplication check and re-embeds every document, even unchanged ones. Set it only on specific documents where you want to reprocess, or after a chunker/embedder change where consistency is required.

**Running `full` pipeline without graph or tree stores.** The `full` template gracefully skips graph and tree stages when the corresponding stores are not wired. This is intentional. If you expect graph or tree ingestion, verify the stores are wired by checking the startup log — it lists which stages are enabled.

**Single active experiment limit.** Only one shadow experiment can be `Active` per collection. Starting a second before the first is promoted or abandoned returns HTTP 409. This is enforced atomically — no TOCTOU window.

**Token embedding dimension mismatch.** The embedder dimension must match what the vector store was initialised with. Arcanum validates this at startup, not at query time. If you change models, you must re-create the collection (which re-creates its index) and re-ingest.
