# Arcanum

**Production-grade Retrieval-Augmented Generation engine written in Rust.**

Arcanum combines five retrieval strategies — dense vector, BM25 lexical, knowledge graph, hierarchical RAPTOR tree, and token-level ColBERT — into a single, enterprise-ready system with pluggable backends, async ingestion pipelines, and a native Model Context Protocol (MCP) interface for AI assistants.

---

## Why Arcanum

Most RAG frameworks are single-strategy wrappers around one vector database. Arcanum is different:

- **Five retrieval strategies in a single orchestrator** — hybrid dense+sparse, graph-aware, and hierarchical retrieval are first-class, not afterthoughts.
- **Hexagonal architecture enforced at the type level** — every storage backend, model provider, and external service is hidden behind a trait. Swap LanceDB for PgVector, Tantivy for an external search service, or Neo4j for an in-memory store with a one-line builder change and zero pipeline rewrites.
- **Compiled, not interpreted** — the Rust runtime eliminates GIL contention, cold-start latency, and memory fragmentation that plague Python RAG stacks under concurrent load.
- **Native MCP server** — Claude and other AI assistants can call `search`, `ingest`, `list_collections`, and `eval_run` over JSON-RPC 2.0 directly, without a separate integration layer.
- **Three runtime tiers** — the same binary runs in `Development` (SQLite, in-memory stores), `Production` (Postgres + LanceDB/Neo4j), or `Enterprise` (full RBAC + audit retention + secret rotation) mode, enforced at startup.

---

## Architecture

```
┌─────────────────────────────────────────────────────────────────────┐
│                          arcanum-server                             │
│     REST /api/v1   ·   Admin /admin/*   ·   WebSocket /ws/events   │
└──────────────────────────────┬──────────────────────────────────────┘
                               │
┌──────────────────────────────▼──────────────────────────────────────┐
│                          arcanum-engine                             │
│   ArcanumEngine: auth · audit · events · circuit breakers           │
│   ┌─────────────────┐  ┌──────────────────┐  ┌──────────────────┐  │
│   │ IngestionService│  │ RetrievalService │  │  AdminService    │  │
│   └────────┬────────┘  └────────┬─────────┘  └──────────────────┘  │
└────────────│────────────────────│────────────────────────────────────┘
             │                    │
┌────────────▼──────┐   ┌────────▼──────────────────────────────────┐
│  arcanum-pipeline │   │            arcanum-retrieval               │
│  DAG stage runner │   │  Orchestrator (Static / QueryClassified /  │
│  ┌─────────────┐  │   │               ParallelFusion)              │
│  │  Templates  │  │   │  ┌──────────┐ ┌──────┐ ┌───────┐ ┌──────┐│
│  │  standard   │  │   │  │  Vector  │ │ BM25 │ │ Graph │ │RAPTOR││
│  │  contextual │  │   │  └──────────┘ └──────┘ └───────┘ └──────┘│
│  │  graph      │  │   │  ┌────────────────────────────────────────┐│
│  │  raptor     │  │   │  │     ColBERT (token-level re-rank)      ││
│  │  full       │  │   │  └────────────────────────────────────────┘│
│  └─────────────┘  │   └───────────────────────────────────────────┘
└───────────────────┘
             │                    │
┌────────────▼────────────────────▼────────────────────────────────────┐
│                           arcanum-core traits                        │
│   VectorStore · GraphStore · TreeStore · Embedder · TextEnricher     │
│   LexicalIndex · GraphPlanner · SecretStore · ProgressEmitter        │
└──────────────────────────────────────────────────────────────────────┘
             │                    │
    ┌────────▼────┐     ┌─────────▼──────────┐     ┌──────────────┐
    │arcanum-vector│     │  arcanum-graph      │     │ arcanum-tree │
    │LanceDB       │     │  Neo4j / in-memory  │     │ RAPTOR tree  │
    │PgVector      │     │  GraphQueryPlanner  │     │ Postgres /   │
    │Tantivy BM25  │     └─────────────────────┘     │ in-memory    │
    └─────────────┘                                  └──────────────┘
```

The 13-crate workspace maps cleanly to layers:

| Layer | Crates |
|---|---|
| **Domain core** | `arcanum-core`, `arcanum-engine` |
| **Ingestion** | `arcanum-ingestion`, `arcanum-pipeline` |
| **Retrieval** | `arcanum-retrieval`, `arcanum-eval` |
| **Storage adapters** | `arcanum-vector`, `arcanum-graph`, `arcanum-tree` |
| **Model adapters** | `arcanum-models` |
| **Infrastructure** | `arcanum-middleware` |
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
| `ParallelFusion` | All retrievers run concurrently; RRF or weighted score fusion |

---

## Ingestion Pipelines

Pipelines are DAGs of typed stages. The stage runner is async and supports concurrent stages with explicit dependency declarations.

### Built-in Templates

| Template | Stages | Use |
|---|---|---|
| `standard` | Load → Preprocess → Chunk → Embed → VectorWrite | Baseline vector RAG |
| `contextual` | + ContextEnrich before Embed | Adds document-level context prefix to each chunk |
| `graph` | + EntityExtract + GraphWrite | Knowledge graph construction alongside vectors |
| `raptor` | + RaptorBuild at the end | Hierarchical tree for summarisation queries |
| `full` | All of the above, conditionally wired | Maximum retrieval coverage |

Stages are opt-in based on what is available on the engine: if no `graph_store` is wired, graph stages are silently skipped. No code changes required to change pipeline behaviour — just wire or omit a dependency in the builder.

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

`build()` validates configuration, spawns the worker pool, wires retrievers based on what is present, and starts the secret-store reload loop. Unrecognised or missing dependencies produce clear errors at startup, not at query time.

### Ingest a document

```rust
engine.ingestion.submit(IngestionTask {
    source_uri: "s3://my-bucket/doc.pdf".into(),
    collection_id: CollectionId("legal".into()),
    pipeline: Some("full".into()),
    force: false,
}).await?;
```

### Search

```rust
let results = engine.retrieval.search(SearchRequest {
    query: "material breach of contract".into(),
    collection_id: CollectionId("legal".into()),
    top_k: 10,
    api_key: token,
}).await?;
```

---

## Enterprise Features

### Authentication & Authorisation

Two token types, one middleware:

- **HMAC API keys** (`HS256`) — issued per user with an `allowed_collections` scope list. `is_admin: true` grants full access.
- **RS256 admin JWTs** — for admin operations; validated against a configurable public key PEM.

All MCP tool calls and admin routes require a `Authorization: Bearer <token>` header. The MCP server performs per-request token extraction — there are no session tokens or shared credentials.

### Role-Based Access Control (RBAC)

Admin operations are gated by a three-tier role hierarchy:

| Role | Capabilities |
|---|---|
| `Tester` | Read health, metrics |
| `Operator` | + Collection management, audit log access, ingestion sources |
| `Admin` | + Key rotation, all destructive operations |

### Audit Logging

Every authenticated operation is recorded with `user_id`, `collection_id`, operation type, result status, and timestamp. The audit trail is queryable via the admin API and retained for a configurable number of days (default 90).

### Circuit Breakers

Independent circuit breakers protect the embedding provider and the vector store. When the failure threshold is exceeded, the breaker opens and requests fail fast with a clear error rather than queuing behind a degraded dependency. Breaker state is tracked per `ArcanumEngine` instance and exposed via health endpoints.

### Secret Store & Hot Reload

`SecretStore` is a trait — back it with HashiCorp Vault, AWS Secrets Manager, or environment variables. `ArcanumEngine` holds the store and spawns a background task that calls `store.reload()` on a configurable interval (default 300 s). The `POST /admin/rotate-keys` endpoint triggers an immediate reload after key rotation so new credentials take effect without a restart.

### CORS

Fail-closed by default: no `Access-Control-Allow-Origin` header is emitted unless `cors_allowed_origins` is explicitly configured. Set via `ARCANUM_CORS_ALLOWED_ORIGINS` environment variable or the `[server]` config section.

```toml
[server]
cors_allowed_origins = ["https://app.example.com", "https://admin.example.com"]
```

### Real-Time Event Bus

WebSocket endpoint at `/ws/events`. Clients subscribe to topics (`ingestion:<collection_id>`, `search:<collection_id>`, `system`). The system topic requires admin role. The event bus is the internal backbone — ingestion workers and the HTTP layer both publish through it, so the WebSocket feed accurately reflects live engine state.

### Admin Portal

Optional built-in admin UI served from `/admin/ui`. Requires `admin.portal_enabled: true` in config. The portal HTML is embedded in the binary — no separate static file server required.

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

Evaluation results surface through the admin API and the MCP `eval_run` tool, making quality regression visible without leaving the development loop.

---

## Configuration Reference

```toml
[global]
runtime_mode = "production"

[ingestion]
worker_pool_size = 8
queue_capacity   = 10000
retry_max_attempts  = 3
retry_base_delay_ms = 1000

[embedding]
provider   = "ollama"
model_id   = "nomic-embed-text"
dimension  = 768
batch_size = 32

[retrieval]
top_k                = 10
orchestration_mode   = "ParallelFusion"
fusion_strategy      = "Rrf"
query_cache_enabled  = true

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
| `arcanum-ingestion` | Loaders, preprocessors (HTML/PDF/EPUB), chunkers |
| `arcanum-middleware` | Circuit breaker, retry policy, bounded queue |
| `arcanum-pipeline` | DAG stage runner and built-in pipeline templates |
| `arcanum-retrieval` | Multi-strategy orchestrator and all Retriever impls |
| `arcanum-eval` | Quality metrics, golden datasets, scheduled evaluation |
| `arcanum-engine` | `ArcanumEngine` builder — wires the full system |
| `arcanum-mcp` | MCP JSON-RPC 2.0 server |
| `arcanum-server` | Axum HTTP server, admin portal, WebSocket handler |

---

## License

MIT — see [LICENSE](LICENSE).
