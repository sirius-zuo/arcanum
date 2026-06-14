# Arcanum

**Production-grade Retrieval-Augmented Generation engine written in Rust.**

Arcanum combines five retrieval strategies — dense vector, BM25 lexical, knowledge graph, hierarchical RAPTOR tree, and token-level ColBERT — into a single, enterprise-ready system with pluggable backends, per-backend chunking strategies, shadow experiment infrastructure, and a native Model Context Protocol (MCP) interface for AI assistants.

---

## Why Arcanum

Most RAG frameworks are single-strategy wrappers around one vector database. Arcanum is different:

- **Five retrieval strategies in a single orchestrator** — hybrid dense+sparse, graph-aware, and hierarchical retrieval are first-class, not afterthoughts.
- **Per-backend chunking** — vector, graph, and tree backends each run their own chunker. A knowledge graph benefits from hierarchical chunks; a vector index benefits from semantic coherence. Both are first-class.
- **Chunk strategy experimentation built-in** — shadow experiments A/B-test a challenger chunking strategy against the live collection without affecting queries. An offline benchmark harness and an inspect API let you measure before you commit.
- **Hexagonal architecture enforced at the type level** — every storage backend, model provider, and external service is hidden behind a trait. Swap LanceDB for PgVector, Tantivy for an external search service, or Neo4j for an in-memory store with a one-line builder change and zero pipeline rewrites.
- **Compiled, not interpreted** — the Rust runtime eliminates GIL contention, cold-start latency, and memory fragmentation that plague Python RAG stacks under concurrent load.
- **Native MCP server** — Claude and other AI assistants can call `search`, `ingest`, `list_collections`, and `eval_run` over JSON-RPC 2.0 directly, without a separate integration layer.
- **Three runtime tiers** — the same binary runs in `Development` (SQLite, in-memory stores), `Production` (Postgres + LanceDB/Neo4j), or `Enterprise` (full RBAC + audit retention + secret rotation) mode, enforced at startup.

---

## Architecture

```
┌────────────────────────────────────────────────────────────────────────┐
│                           arcanum-server                               │
│    REST /api/v1   ·   Admin /admin/*   ·   WebSocket /ws/events        │
└───────────────────────────────┬────────────────────────────────────────┘
                                │
┌───────────────────────────────▼────────────────────────────────────────┐
│                           arcanum-engine                               │
│   ArcanumEngine: auth · audit · events · circuit breakers              │
│   ┌─────────────────┐  ┌──────────────────┐  ┌──────────────────────┐  │
│   │ IngestionService│  │ RetrievalService │  │  ExperimentService   │  │
│   └────────┬────────┘  └────────┬─────────┘  └──────────────────────┘  │
└────────────│────────────────────│────────────────────────────────────────┘
             │                    │
┌────────────▼──────┐   ┌────────▼──────────────────────────────────────┐
│  arcanum-pipeline │   │            arcanum-retrieval                   │
│  DAG stage runner │   │  Orchestrator (Static / QueryClassified /      │
│  ┌─────────────┐  │   │               ParallelFusion)                  │
│  │  Templates  │  │   │  ┌──────────┐ ┌──────┐ ┌───────┐ ┌──────┐    │
│  │  standard   │  │   │  │  Vector  │ │ BM25 │ │ Graph │ │RAPTOR│    │
│  │  contextual │  │   │  └──────────┘ └──────┘ └───────┘ └──────┘    │
│  │  graph      │  │   │       document-level RRF fusion                │
│  │  raptor     │  │   └───────────────────────────────────────────────┘
│  │  full       │  │
│  └─────────────┘  │
│  per-backend      │   ┌───────────────────────────────────────────────┐
│  chunkers:        │   │           arcanum-chunk-eval                   │
│  vector_chunk     │   │  Inspect API · Offline Benchmark · Experiments │
│  graph_chunk      │   └───────────────────────────────────────────────┘
│  tree_chunk       │
└───────────────────┘
             │
┌────────────▼────────────────────────────────────────────────────────────┐
│                          arcanum-core traits                             │
│   VectorStore · GraphStore · TreeStore · Embedder · TextEnricher        │
│   LexicalIndex · GraphPlanner · SecretStore · DocumentRegistry          │
└─────────────────────────────────────────────────────────────────────────┘
             │
    ┌────────▼─────┐    ┌──────────────────────┐    ┌──────────────┐
    │arcanum-vector│    │   arcanum-graph        │    │ arcanum-tree │
    │LanceDB       │    │   Neo4j / in-memory    │    │ RAPTOR tree  │
    │PgVector      │    │   GraphQueryPlanner    │    │ Postgres /   │
    │Tantivy BM25  │    └────────────────────────┘    │ in-memory    │
    └──────────────┘                                  └──────────────┘
```

The 15-crate workspace maps cleanly to layers:

| Layer | Crates |
|---|---|
| **Domain core** | `arcanum-core`, `arcanum-engine` |
| **Ingestion** | `arcanum-ingestion`, `arcanum-pipeline` |
| **Retrieval** | `arcanum-retrieval`, `arcanum-eval` |
| **Chunk evaluation** | `arcanum-chunk-eval` |
| **Storage adapters** | `arcanum-vector`, `arcanum-graph`, `arcanum-tree` |
| **Model adapters** | `arcanum-models` |
| **Infrastructure** | `arcanum-middleware`, `arcanum-telemetry` |
| **Interfaces** | `arcanum-server`, `arcanum-mcp` |

---

## Retrieval Strategies

### 1 — Vector (Dense ANN)
Embeds the query and performs approximate nearest-neighbour search over stored chunk vectors. Default backend is LanceDB; PgVector is supported for teams already running Postgres.

### 2 — BM25 (Lexical)
Full-text retrieval via an embedded Tantivy engine. Collection-isolated: each `Bm25Retriever` instance is scoped to a single collection, preventing cross-collection data leakage at the type level.

### 3 — Graph-Augmented
Extracts entity names from the query, traverses a knowledge graph (Neo4j or in-memory) up to a configurable hop depth, and then performs a vector search filtered to the retrieved entity contexts. Effective for relationship-heavy domains.

### 4 — RAPTOR (Hierarchical Tree)
Builds a recursive summarisation tree over ingested chunks using K-means clustering. At query time, traversal spans all levels (coarse-to-fine), with level-weighted cosine scoring. Handles abstractive questions that require document-level reasoning, not just chunk-level matches.

### 5 — ColBERT (Token-Level Re-Ranking)
Performs a coarse ANN pass followed by a MaxSim token-vector re-rank. Falls back gracefully to coarse scores when token vectors are absent. Provides the precision of cross-encoder models at closer-to-bi-encoder latency.

### Orchestration Modes

| Mode | Behaviour |
|---|---|
| `Static` | Fixed retriever order, first result set returned |
| `QueryClassified` | Classifier routes queries to the most relevant retriever |
| `ParallelFusion` | All retrievers run concurrently; document-level RRF fusion |

**Fusion key:** Arcanum keys cross-backend fusion on `document_id`, not `chunk_id`. With per-backend chunkers each backend produces independent `ChunkId`s that never align — `document_id` is the stable, correct cross-backend key. A document appearing in both vector and graph results is boosted; one appearing in only one is not penalised.

---

## Ingestion Pipelines

Pipelines are DAGs of typed stages. The stage runner is async and supports concurrent stages with explicit dependency declarations.

### Document Preprocessing

Before chunking, every document passes through a preprocessor chain that converts raw bytes to clean text. Arcanum ships two preprocessor sets:

| Preprocessor set | Covered MIME types | When used |
|---|---|---|
| `default_chains` (built-in) | PDF, HTML, XHTML, EPUB, DOCX | No `[ingestion.docling]` config |
| `DoclingPreprocessor` (docling) | PDF, DOCX, PPTX, XLSX, EPUB, HTML, XHTML, PNG, JPEG, TIFF | `[ingestion.docling]` present in config |

`DoclingPreprocessor` integrates with [docling-serve](https://github.com/DS4SD/docling-serve) and extends format coverage to presentations, spreadsheets, and images — formats the built-in parsers do not handle.

**HTTP backend** — posts each document to a running docling-serve instance. Supports synchronous and asynchronous (poll-based) modes:

```toml
[ingestion.docling.backend]
type             = "http"
base_url         = "http://docling-serve:5001"
timeout_secs     = 300
use_async        = true   # poll for completion instead of blocking
poll_interval_ms = 2000
```

**CLI backend** — shells out to a local `docling` binary. Useful for air-gapped environments or local development without a server:

```toml
[ingestion.docling.backend]
type    = "cli"
command = "docling"
```

When `[ingestion.docling]` is absent, the engine falls back to `default_chains`, which covers the five most common formats without any external dependency.

### Per-Backend Chunking

Every pipeline template runs three independent chunking branches from the same preprocessed document:

```
Preprocess ─┬─→ vector_chunk (chunkers.vector) → embed → vector_write
            ├─→ graph_chunk  (chunkers.graph)  → entity_extract → graph_write
            └─→ tree_chunk   (chunkers.tree)   → tree_embed → raptor_build → tree_write
```

Each branch uses its own `Arc<dyn Chunker>` resolved from a two-tier config:

1. **Collection-level override** — per-collection `PerBackendChunkConfig`, set at creation or updated via the collection API.
2. **Global default** — `IngestionConfig.chunking`, applied when the collection has no override.

Both paths go through `ChunkRegistry.build()` at job-start time — a bad config fails immediately, not mid-ingest.

### Built-in Chunking Strategies

| Strategy | Key parameter | Best for |
|---|---|---|
| `fixed` | `chunk_size`, `overlap` | Predictable token budgets |
| `semantic` | `max_chars` | Sentence-boundary-aware splitting |
| `propositional` | — | Claim-level granularity |
| `hierarchical` | — | Knowledge graph construction |
| `structure_aware` | — | Markdown / HTML with heading structure |

Specify a strategy in config or per-collection override:

```json
{
  "vector": { "strategy": "semantic", "params": { "max_chars": 800 } },
  "graph":  { "strategy": "hierarchical", "params": {} },
  "tree":   { "strategy": "fixed", "params": { "chunk_size": 1024, "overlap": 128 } }
}
```

### Built-in Pipeline Templates

| Template | Stages | Use |
|---|---|---|
| `standard` | Load → Dedup → Cleanup → Preprocess → (vector/graph/tree)_chunk → Embed → VectorWrite | Baseline vector RAG |
| `contextual` | + ContextEnrich before Embed | Adds document-level context prefix to each chunk |
| `graph` | + EntityExtract + GraphWrite | Knowledge graph alongside vectors |
| `raptor` | + TreeEmbed + RaptorBuild | Hierarchical tree for summarisation queries |
| `full` | All of the above, conditionally wired | Maximum retrieval coverage |

Stages are opt-in based on what is present on the engine. If no `graph_store` is wired, graph stages are silently skipped. No code changes required — just wire or omit a dependency in the builder.

### Document Deduplication

A `DocumentRegistry` tracks each `(source_uri, collection_id)` pair. On re-ingest, the pipeline compares content hashes and skips unchanged documents, replaces changed ones (cleaning stale chunks first), and handles interrupted-cleanup recovery via a `Replacing` status.

---

## Chunk Strategy Evaluation (`arcanum-chunk-eval`)

Three tools for measuring and improving chunking quality before committing to a strategy in production.

### A — Inspect API (stateless)

Compare multiple strategies on any text blob. No storage, no embeddings, pure CPU:

```http
POST /api/v1/chunk/inspect
Content-Type: application/json

{
  "text": "The transformer architecture was introduced...",
  "strategies": [
    { "strategy": "fixed",    "params": { "chunk_size": 512, "overlap": 64 } },
    { "strategy": "semantic", "params": { "max_chars": 800 } }
  ]
}
```

Returns per-strategy `total_chunks`, `mean_tokens`, per-chunk `char_count`, `token_estimate`, and `overlap_chars`.

### B — Offline Benchmark Harness

Submit a labeled corpus and get recall metrics back:

```http
POST /api/v1/chunk/benchmark
Authorization: Bearer <token>
Content-Type: application/json

{
  "corpus": [{ "source_uri": "doc1", "content": "..." }],
  "queries": [{ "text": "...", "expected_doc_ids": ["doc1"] }],
  "strategies": [
    { "vector": { "strategy": "fixed",    "params": { "chunk_size": 512, "overlap": 64 } } },
    { "vector": { "strategy": "semantic", "params": { "max_chars": 800 } } }
  ]
}
```

Returns `recall_at_5`, `recall_at_10`, `mean_chunk_tokens`, `chunk_size_p50`, `chunk_size_p95` per strategy. No LLM calls — recall against labeled document IDs is the signal.

### C — Shadow Experiments (live A/B testing)

Test a challenger strategy on real traffic without affecting queries:

```http
# Start experiment — all new documents are also written to a shadow namespace
POST /api/v1/collections/{id}/experiments
{ "vector": { "strategy": "semantic", "params": { "max_chars": 800 } }, "graph": null, "tree": null }

# Poll status and metrics
GET /api/v1/collections/{id}/experiments/{exp_id}

# When challenger_recall_at_5 leads by ≥5% over ≥50 documents → ReadyToPromote
# Promote: collection's chunker_config updated; new documents use promoted strategy
POST /api/v1/collections/{id}/experiments/{exp_id}/promote

# Or abandon (config unchanged)
DELETE /api/v1/collections/{id}/experiments/{exp_id}
```

Shadow writes are best-effort — a shadow write failure never fails the primary ingestion job. Only the primary namespace is queried. At most one `Active` experiment per collection at a time.

---

## Getting Started

```rust
use arcanum_engine::ArcanumEngine;
use arcanum_core::config::ArcanumConfig;
use std::sync::Arc;

// Minimal — vector search only
let engine = ArcanumEngine::builder()
    .auth_secret("your-32-char-minimum-secret-here")
    .vector_store(Arc::new(my_lance_db_store))
    .embedder(Arc::new(my_ollama_embedder))
    .build()
    .await?;

// Full — all retrieval strategies
let engine = ArcanumEngine::builder()
    .config(ArcanumConfig::from_env())
    .auth_secret(std::env::var("ARCANUM_AUTH_SECRET")?)
    .vector_store(Arc::new(lance_store))
    .embedder(Arc::new(ollama_embedder))
    .enricher(Arc::new(llm_enricher))         // enables contextual + graph stages
    .graph_store(Arc::new(neo4j_store))       // enables graph retrieval
    .tree_store(Arc::new(raptor_pg_store))    // enables RAPTOR retrieval
    .secret_store(Arc::new(vault_store))      // enables hot-reload
    .build()
    .await?;
```

`build()` validates configuration, spawns the worker pool with per-job chunker resolution, wires retrievers based on what is present, and starts background tasks (secret reload, experiment eval loop). Unrecognised or missing dependencies produce clear errors at startup, not at query time.

### Ingest a document

```rust
// Via Rust API
engine.ingestion.ingest(IngestRequest {
    source_uri: "s3://my-bucket/doc.pdf".into(),
    collection_id: CollectionId("legal".into()),
    pipeline_template: Some("full".into()),
    force: false,
    content: None,
    mime_hint: None,
}, &user_id).await?;

// Via HTTP — URI reference
curl -X POST /api/v1/ingest \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"source_uri":"s3://bucket/doc.pdf","collection_id":"legal","pipeline":"full"}'

// Via HTTP — direct upload
curl -X POST "/api/v1/upload?collection_id=legal&filename=contract.pdf&pipeline=full" \
  -H "Authorization: Bearer $TOKEN" \
  --data-binary @contract.pdf
```

### Search

```rust
let results = engine.retrieval.search(
    Query::new("material breach of contract")
        .with_collection(CollectionId("legal".into()))
        .with_top_k(10),
    &claims,
).await?;
```

---

## Enterprise Features

### Authentication & Authorisation

Two token types, one middleware:

- **HMAC API keys** (`HS256`) — issued per user with an `allowed_collections` scope list. `is_admin: true` grants full access.
- **RS256 admin JWTs** — for admin operations; validated against a configurable public key PEM.

All MCP tool calls and admin routes require an `Authorization: Bearer <token>` header. The MCP server performs per-request token extraction — there are no session tokens or shared credentials.

### Role-Based Access Control (RBAC)

Admin operations are gated by a three-tier role hierarchy:

| Role | Capabilities |
|---|---|
| `Tester` | Read health, metrics |
| `Operator` | + Collection management, audit log access, ingestion sources, chunk experiments |
| `Admin` | + Key rotation, all destructive operations |

### Audit Logging

Every authenticated operation is recorded with `user_id`, `collection_id`, operation type, result status, and timestamp. The audit trail is queryable via the admin API and retained for a configurable number of days (default 90).

### Circuit Breakers

Independent circuit breakers protect the embedding provider and the vector store. When the failure threshold is exceeded, the breaker opens and requests fail fast with a clear error rather than queuing behind a degraded dependency. Shadow writes respect the vector store circuit breaker and skip rather than block.

### Secret Store & Hot Reload

`SecretStore` is a trait — back it with HashiCorp Vault, AWS Secrets Manager, or environment variables. `ArcanumEngine` holds the store and spawns a background task that calls `store.reload()` on a configurable interval (default 300 s). `POST /admin/rotate-keys` triggers an immediate reload after key rotation.

### CORS

Fail-closed by default: no `Access-Control-Allow-Origin` header is emitted unless `cors_allowed_origins` is explicitly configured.

```toml
[server]
cors_allowed_origins = ["https://app.example.com", "https://admin.example.com"]
```

### Real-Time Event Bus

WebSocket endpoint at `/ws/events`. Clients subscribe to topics (`ingestion:<collection_id>`, `search:<collection_id>`, `system`). The system topic requires admin role.

### Observability

`arcanum-telemetry` provides structured tracing (OpenTelemetry-compatible), Prometheus-compatible metrics, and a pre-built Grafana dashboard stack. Key metrics:

| Metric | Type | Description |
|---|---|---|
| `arcanum_requests_total` | Counter | Per-endpoint request counts with status label |
| `arcanum_request_duration_seconds` | Histogram | Per-endpoint latency |
| `arcanum_ingest_docs_total` | Counter | Documents ingested, by status |
| `arcanum_active_retrievers` | Gauge | Number of wired retriever strategies |

---

## Runtime Modes

```toml
[global]
runtime_mode = "enterprise"   # development | production | enterprise
```

| Mode | Metadata backend | Enforcement |
|---|---|---|
| `development` | SQLite permitted | None — suitable for local iteration |
| `production` | Postgres required | SQLite rejected at startup |
| `enterprise` | Postgres required | Postgres + audit retention + IP allowlist + admin JWT |

Mode is also readable from the `ARCANUM_RUNTIME_MODE` environment variable. Config is layered: defaults → file (`config.toml` or `config.yaml`) → environment variables, with later layers taking precedence.

---

## MCP Integration

Arcanum ships a JSON-RPC 2.0 MCP server (`arcanum-mcp`). Claude and other AI assistants can call the following tools directly:

| Tool | Parameters | Returns |
|---|---|---|
| `search` | `query`, `collection_id`, `top_k` | Array of scored chunks |
| `ingest` | `source_uri`, `collection_id`, `pipeline` | `operation_id` for tracking |
| `list_collections` | — | Collection metadata array |
| `eval_run` | `collection_id` | Evaluation metrics (MRR, NDCG@k, hit rate) |

Every tool call requires a valid Bearer token. The MCP server validates the token against `engine.auth` on each request — no shared session, no bypass.

---

## Retrieval Quality Evaluation

`arcanum-eval` provides continuous measurement of retrieval quality against golden datasets:

- **Metrics**: MRR, NDCG@k, Hit Rate@k
- **Scheduling**: Cron-based via `eval.schedule_cron` in config
- **Datasets**: `BenchmarkDataset` abstraction supports golden sample ingestion and programmatic querying

---

## Configuration Reference

```toml
[global]
runtime_mode = "production"

[ingestion]
worker_pool_size    = 8
queue_capacity      = 10000
retry_max_attempts  = 3
retry_base_delay_ms = 1000

# Docling preprocessor — omit this section to use the built-in parsers
[ingestion.docling.backend]
type             = "http"
base_url         = "http://docling-serve:5001"
timeout_secs     = 300
use_async        = false

# Global default chunker — per-collection overrides take precedence
[ingestion.chunking.vector]
strategy = "semantic"
params   = { max_chars = 800 }

# graph and tree default to vector when not specified
[ingestion.chunking.graph]
strategy = "hierarchical"
params   = {}

[embedding]
provider   = "ollama"
model_id   = "nomic-embed-text"
dimension  = 768
batch_size = 32

[retrieval]
top_k               = 10
orchestration_mode  = "ParallelFusion"
fusion_strategy     = "Rrf"
query_cache_enabled = true

[storage]
metadata_backend = "postgres"
vector_backend   = "lancedb"
graph_enabled    = true
tree_enabled     = true

[admin]
portal_enabled                    = true
audit_retention_days              = 90
secret_store_reload_interval_secs = 300
jwt_rs256_public_key_pem          = "-----BEGIN PUBLIC KEY-----\n..."

[server]
cors_allowed_origins = ["https://app.example.com"]
```

All values are overridable via environment variables prefixed with `ARCANUM_`.

---

## Workspace Crates

| Crate | Description |
|---|---|
| `arcanum-core` | Shared traits, types, config, and error types |
| `arcanum-vector` | LanceDB, PgVector, and Tantivy BM25 adapters |
| `arcanum-graph` | Neo4j driver and in-memory graph store |
| `arcanum-tree` | RAPTOR tree builder, Postgres and in-memory stores |
| `arcanum-models` | HTTP embedding clients (Ollama, OpenAI), Redis cache |
| `arcanum-ingestion` | Loaders, preprocessors (HTML/PDF/EPUB/DOCX + DoclingPreprocessor for PPTX/XLSX/images), chunkers, ChunkRegistry |
| `arcanum-middleware` | Circuit breaker, retry policy, bounded queue |
| `arcanum-pipeline` | DAG stage runner and built-in pipeline templates |
| `arcanum-retrieval` | Multi-strategy orchestrator and all Retriever impls |
| `arcanum-eval` | Quality metrics, golden datasets, scheduled evaluation |
| `arcanum-chunk-eval` | Chunk inspect API, offline benchmark harness, shadow experiment evaluation |
| `arcanum-engine` | `ArcanumEngine` builder — wires the full system |
| `arcanum-mcp` | MCP JSON-RPC 2.0 server |
| `arcanum-server` | Axum HTTP server, admin portal, WebSocket handler |
| `arcanum-telemetry` | Structured tracing, Prometheus metrics, Grafana stack |

---

## License

MIT — see [LICENSE](LICENSE).
