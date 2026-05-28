# Arcanum RAG Framework — Architecture Design

**Version:** 2.0  
**Date:** 2026-05-28  
**Status:** Approved for implementation planning  
**Supersedes:** architecture.html v1.2

---

## 1. Positioning

**Arcanum** is a high-performance Rust RAG knowledge engine framework. It handles the full knowledge lifecycle: ingestion, enrichment, indexing, and retrieval. It is the foundational knowledge layer for **ArcVerse** and integrates with any agent framework via the MCP protocol or HTTP/gRPC.

Arcanum is explicitly **not** an agent framework. The boundary is:

- **Arcanum owns:** anything that uses models or ML to improve the quality, structure, or retrievability of knowledge — contextual enrichment, entity extraction, hierarchical clustering, retrieval fusion, quality evaluation.
- **ArcVerse owns:** multi-step reasoning, tool use, memory management, task planning.

ArcVerse consumes `RetrievalResult` from Arcanum. It has no knowledge of how that result was produced.

### Design Principles

- Trait-driven, not inheritance-driven — every component is swappable
- Library-first — usable as a Rust crate before it is a service
- Named templates, composable internals — sensible defaults, full flexibility underneath
- Measure everything — evaluation is first-class, not an afterthought
- Provider-agnostic — local and cloud models, any vector store, any graph backend
- Anti-corruption boundaries — external API types never leak past their crate

---

## 2. Crate Architecture

Thirteen crates in four layers. Each crate depends only on crates in layers above it. No cycles.

```
── Foundation ──────────────────────────────────────────────────────
  arcanum-core          Traits, domain types, config, errors

── Storage & Models ────────────────────────────────────────────────
  arcanum-vector        VectorStore + BM25 + MetadataStore impls
  arcanum-graph         GraphStore impls (entity/relationship index)
  arcanum-tree          TreeStore impls + RAPTOR algorithm
  arcanum-models        Embedder + TextEnricher provider impls

── Processing ──────────────────────────────────────────────────────
  arcanum-ingestion     Document loaders, chunkers, enrichment stages
  arcanum-retrieval     RetrievalOrchestrator, strategies, fusion, reranking
  arcanum-eval          Metrics, benchmark runner, quality evaluation
  arcanum-pipeline      DAG execution engine, named pipeline templates
  arcanum-middleware    Circuit breakers, bounded queue, retry

── Service ─────────────────────────────────────────────────────────
  arcanum-engine        ArcanumEngine, service handlers, auth, audit
  arcanum-mcp           MCP transport (JSON-RPC over SSE/WebSocket)
  arcanum-server        HTTP/gRPC transport + admin portal
```

**Dependency graph:**

```
arcanum-core
    ↑
arcanum-vector   arcanum-graph   arcanum-tree   arcanum-models
    ↑
arcanum-ingestion   arcanum-retrieval   arcanum-eval
    ↑
arcanum-pipeline   arcanum-middleware
    ↑
arcanum-engine
    ↑                  ↑
arcanum-mcp        arcanum-server
```

`arcanum-mcp` and `arcanum-server` are transport peers — neither depends on the other. Both consume `arcanum-engine` for all business logic.

---

## 3. Runtime Workflows

### 3.1 Service Startup

```
ArcanumConfig (loaded from file / env / secrets)
    ↓
ArcanumEngine::build()
    ├── connects to arcanum-vector (pgvector / LanceDB)
    ├── connects to arcanum-graph (Neo4j / Kuzu) — if enabled
    ├── connects to arcanum-tree (RAPTOR index) — if enabled
    ├── initializes arcanum-models (Embedder + TextEnricher providers)
    ├── registers pipeline templates (Standard, Contextual, RAPTOR, Graph, Full)
    ├── initializes RetrievalOrchestrator with enabled strategies
    ├── initializes arcanum-eval (if enabled)
    └── starts IngestionWorker pool (tokio background tasks)
          ↓
McpServer::new(engine.clone())   HttpServer::new(engine.clone())
    both start, both share the same ArcanumEngine instance
```

`ArcanumEngine` is built once and shared via `Arc<ArcanumEngine>`. Neither transport layer owns business logic.

### 3.2 Ingestion Request Lifecycle

```
Client → POST /ingest  (or MCP tool: ingest)
           ↓
    arcanum-server / arcanum-mcp
    (validate auth, rate-limit — delegated to arcanum-engine)
           ↓
    IngestionService (in arcanum-engine)
    ├── validates request
    ├── checks DocumentHashTracker → skip if unchanged
    ├── creates IngestionTask { source, pipeline_template, collection_id, op_id }
    └── pushes to BoundedQueue (arcanum-middleware)
           ↓
    returns OperationId immediately
           ↓  (background)
    IngestionWorker (tokio task pool, arcanum-pipeline)
    └── selects pipeline DAG from template
           ↓
    Pipeline DAG execution (arcanum-pipeline)

    ┌─ Stage: DocumentLoader (arcanum-ingestion)
    ├─ Stage: Preprocessor (arcanum-ingestion)
    │
    │   ┌── if GraphPipeline or FullPipeline ─────────────────┐
    ├─ Stage: EntityExtractor (arcanum-ingestion + arcanum-models)
    │   └─ Stage: GraphWriter → arcanum-graph ────────────────┘
    │
    ├─ Stage: Chunker (arcanum-ingestion)
    │
    │   ┌── if ContextualPipeline or FullPipeline ────────────┐
    ├─ Stage: ContextEnricher (arcanum-ingestion + arcanum-models)
    │   └─────────────────────────────────────────────────────┘
    │
    ├─ Stage: Embedder (arcanum-models)
    ├─ Stage: VectorWriter → arcanum-vector
    │
    │   ┌── if RAPTORPipeline or FullPipeline ────────────────┐
    └─ Stage: RAPTORBuilder (arcanum-tree + arcanum-models)
        └─────────────────────────────────────────────────────┘
           ↓
    IngestionReport generated → status updated → WebSocket event emitted
```

Independent DAG branches (GraphWrite and VectorWrite) run concurrently. A branch failure produces `PartialSuccess`, not `Failed`, unless a core stage (Chunk, Embed, VectorWrite) failed.

### 3.3 Retrieval Request Lifecycle

```
Client → POST /search  (or MCP tool: search)
           ↓
    arcanum-server / arcanum-mcp
    (validate auth, rate-limit)
           ↓
    RetrievalService (in arcanum-engine)
    └── checks QueryCache → return cached result if hit
           ↓ (cache miss)
    QueryTransformer (arcanum-retrieval)
    └── HyDE / MultiQuery / QueryRewrite — configurable, composable
           ↓
    RetrievalOrchestrator (arcanum-retrieval)

    ┌── parallel, with per-strategy timeout ──────────────────────────┐
    │  VectorRetriever     → arcanum-vector (ANN search)               │
    │  BM25Retriever       → arcanum-vector (keyword index)            │
    │  ColBERTRetriever    → arcanum-vector (MaxSim token scoring)     │
    │  RAPTORRetriever     → arcanum-tree   (multi-level tree query)   │
    │  GraphRetriever      → arcanum-graph  (entity + relation graph)  │
    └─────────────────────────────────────────────────────────────────┘
           ↓ (partial results valid if a strategy timed out)
    FusionEngine — RRF / WeightedFusion / LearnedFusion
           ↓
    Reranker — CrossEncoder / LLMReranker / ScoreFusion / None
           ↓
    ResultProcessor — Deduplication + CitationGenerator
           ↓
    QueryCache stores result
           ↓
    RetrievalResult { chunks, citations, strategy_scores, confidence }
```

`strategy_scores` exposes per-chunk contribution by strategy. ArcVerse and external agents consume this as a confidence signal.

---

## 4. arcanum-core

Foundation layer. Minimal dependencies. No network calls. No business logic.

### 4.1 Trait Roster

| Trait | Signature | Purpose |
|---|---|---|
| `DocumentLoader` | `Source → RawDocument` | Load bytes from any source |
| `Preprocessor` | `RawDocument → RawDocument` | Clean, normalize, extract structure |
| `Chunker` | `RawDocument → Vec<Chunk>` | Split into indexable units |
| `TextEnricher` | `EnrichRequest → EnrichedText` | Context prefix, summarization, entity extraction |
| `Embedder` | `Vec<String> → Vec<Vector>` | Dense vector generation |
| `VectorStore` | `Chunk/Query → CRUD + ANN` | Flat vector index operations |
| `GraphStore` | `Entity/Relation → CRUD + Traversal` | Knowledge graph operations |
| `TreeStore` | `TreeNode → CRUD + Level query` | Hierarchical index operations |
| `Retriever` | `Query → Vec<ScoredChunk>` | Single-strategy retrieval |
| `Reranker` | `(Query, Vec<Chunk>) → Vec<ScoredChunk>` | Reorder by relevance |
| `Evaluator` | `(Query, Vec<Chunk>, Groundtruth) → Metrics` | Quality measurement |
| `SecretStore` | `KeyPath → SecretValue` | Credential loading abstraction |

**`TextEnricher` contract:**

```rust
async fn enrich(&self, request: EnrichRequest) -> Result<EnrichedText>;

// EnrichRequest carries:
//   - text: the content to enrich
//   - intent: ContextPrefix | Summarize | ExtractEntities | Caption | Rerank | Custom
//   - context: optional document title, section, adjacent chunks
```

The `intent` field lets a single provider dispatch correctly without callers constructing prompts. Prompts are owned by the provider implementation, configurable via `EnrichmentConfig`.

### 4.2 Core Domain Types

| Type | Description |
|---|---|
| `RawDocument` | Source bytes + origin metadata (URI, content type, hash) |
| `Chunk` | Text segment + position + metadata + parent document ref |
| `IndexedChunk` | Chunk + vector(s) + store ID |
| `RetrievedChunk` | IndexedChunk + relevance score + `strategy: RetrievalStrategy` |
| `Query` | Query text + filters + collection scope + retrieval config override |
| `MetadataFilter` | Field, operator, value — pre-filter for VectorStore |
| `Entity` | Name, type, canonical ID, source chunk refs |
| `Relation` | (Entity, RelationType, Entity) + confidence + source chunk |
| `TreeNode` | Text/summary + level + children refs + cluster centroid + vector |
| `OperationId` | Ingestion job handle for status polling |
| `RetrievalResult` | Vec<RetrievedChunk> + citations + strategy_scores + confidence |
| `IngestionReport` | Per-stage counts, errors, duration, document fingerprint |

### 4.3 Configuration System

Layered — each layer overrides the previous:

1. Built-in defaults (compiled in)
2. Config file (TOML / YAML)
3. Environment variables (`ARCANUM_*`)
4. Runtime overrides (admin API, hot-reload)

`ArcanumConfig` sections: `GlobalConfig`, `IngestionConfig`, `EmbeddingConfig`, `EnrichmentConfig`, `StorageConfig`, `RetrievalConfig`, `EvalConfig`, `AdminConfig`.

### 4.4 Anti-Corruption Layer

External API types (vector store responses, embedding provider errors, graph DB wire formats) never leave their crate. Each store and model crate defines adapter functions at the crate boundary. The pipeline and engine layers see only `Chunk`, `Vector`, `Entity`, `RetrievedChunk` — never `QdrantScoredPoint` or `Neo4jRecord`.

### 4.5 Cross-Cache Invalidation Protocol

Defined at the `arcanum-core` level as a cross-module contract:

```
document modified
    → (1) EmbeddingCache: invalidate vector entries for that document's chunks
    → (2) QueryCache: invalidate entries for affected collections
```

Implemented as a single atomic operation in the pipeline layer. This is a contract, not an implementation detail — both caches must honor it.

---

## 5. arcanum-vector

Flat persistence: dense vectors, sparse BM25 index, document/chunk metadata. All three share the same primary key space (`ChunkId`).

### Backends

| Component | Development | Production | Enterprise |
|---|---|---|---|
| VectorStore | LanceDB (local file) | pgvector (PostgreSQL) | Qdrant / Weaviate |
| BM25 index | Tantivy (embedded) | Tantivy (embedded) | Tantivy or Elasticsearch |
| MetadataStore | SQLite | PostgreSQL | PostgreSQL (replicas) |

**SQLite restriction:** Development and prototyping only. `ArcanumEngine::build()` rejects SQLite when `GlobalConfig.runtime_mode = Production` or `Enterprise`.

### Key Components

- **VectorStore trait impls** — LanceDB, pgvector, Qdrant, Weaviate
- **BM25Index** — Tantivy-backed, written in parallel with embedding during ingestion
- **MetadataStore** — documents, chunks, collections, ingestion operations, document hashes
- **CollectionManager** — named collections with independent vector spaces and metadata
- **HybridIndexManager** — coordinates VectorStore + BM25Index writes atomically
- **Multi-vector support (ColBERT)** — token-level vectors stored as payload alongside primary vector; backends without native multi-vector support serialize as a column with fallback to single-vector scoring

---

## 6. arcanum-graph

Entity and relationship index. Fully independent of `arcanum-vector`.

### Backends

`GraphStore` trait impls: Neo4j, Apache AGE (PostgreSQL extension), Kuzu (embedded, development), SurrealDB.

### Key Components

- **GraphStore trait impls** — CRUD for `Entity` and `Relation` + graph traversal
- **EntityIndex** — lookup by name, type, canonical ID; maps entity to source chunk IDs
- **RelationIndex** — traversal: entity → connected entities within N hops, filtered by relation type
- **GraphQueryPlanner** — translates a natural-language entity query into a traversal plan

`arcanum-graph` does not extract entities — that is `arcanum-ingestion`'s job via `TextEnricher`. Each `Entity` carries `Vec<ChunkId>` linking back to source chunks, enabling `GraphRetriever` to return `RetrievedChunk` records with `strategy: Graph` that participate in fusion identically to vector results.

---

## 7. arcanum-tree

RAPTOR hierarchical index. Stores a tree of chunks and cluster summaries built at ingestion time, queryable at multiple granularity levels.

### Backends

`TreeStore` trait impls: Kuzu (native graph structure, development), PostgreSQL with adjacency table (production).

### Key Components

- **RAPTORBuilder** — ingestion-time algorithm:
  1. Embed leaf chunks (delegates to `arcanum-models`)
  2. Cluster by semantic similarity (Gaussian Mixture Model)
  3. `TextEnricher(intent=Summarize)` generates summary for each cluster
  4. Embed summaries → new tree level
  5. Repeat until single root or configured max depth (default: 3)

- **RAPTORRetriever** — queries leaf, mid, and root levels in parallel; merges across levels (leaf results favored for specific queries, root results favored for broad analytical queries)
- **TreeNode** — `{ id, level, text, vector, children, parent, cluster_centroid }`

RAPTOR is a static index — the tree is built at ingestion time. ArcVerse consumes `RetrievedChunk` results; it has no knowledge of whether a result came from a leaf node or a cluster summary.

---

## 8. arcanum-models

Unified model provider crate. `arcanum-core` defines the traits; `arcanum-models` provides implementations.

### Provider Matrix

| Provider | Embedder | TextEnricher | Notes |
|---|---|---|---|
| Ollama | ✓ | ✓ | Single local deployment serves both |
| OpenAI | ✓ | ✓ | Separate embedding + chat endpoints |
| Anthropic (Claude) | — | ✓ | No embedding API |
| HuggingFace TEI | ✓ | — | Embedding service only |
| BGE / E5 (local) | ✓ | — | Encoder-only models |
| Mistral | ✓ | ✓ | Embedding + chat API |
| LLM2Vec | ✓ | ✓ | Decoder LLM repurposed for both roles |
| GLiNER (local) | — | ✓ | Lightweight entity extraction, no LLM needed |
| spaCy (local) | — | ✓ | Rule/model NLP, no LLM needed |

A single Ollama deployment with Qwen2.5 satisfies both `Embedder` and `TextEnricher` for a full local setup with zero external dependencies. Different `TextEnricher` intents can route to different providers (e.g., GLiNER for entity extraction, Claude for context prefix generation).

### Key Components

- **EmbeddingParallelismRouter** — round-robin for TEI/local instances; batch-split with rate-aware pacing for cloud providers
- **EmbeddingCache** — optional Redis-backed cache keyed on `(text_hash, model_id, dimension)`; participates in cross-cache invalidation protocol
- **EnrichmentDispatcher** — routes `TextEnricher` calls by intent; each intent maps to a configurable prompt template owned by the provider
- **ProviderHealthMonitor** — tracks provider latency and error rates; feeds into circuit breakers

---

## 9. arcanum-ingestion

Document processing stage implementations. Does not execute the pipeline DAG — that is `arcanum-pipeline`'s job.

### Stage Implementations

**Document loaders:** `FileLoader` (PDF, Markdown, DOCX, ePub, plain text), `UrlLoader`, `DatabaseLoader` (PostgreSQL, MySQL, SQLite), `NotionLoader`, `ConfluenceLoader`, `S3Loader`, `GCSLoader`

**Preprocessors:** `HtmlCleaner`, `TableExtractor`, `ImageCaptioner` (delegates to `TextEnricher(intent=Caption)`), `LanguageDetector`

**Chunkers:**
- `FixedSizeChunker` — character/token count with overlap
- `SemanticChunker` — splits at semantic boundaries using embedding similarity
- `HierarchicalChunker` — respects document structure (headers, sections, paragraphs)
- `StructureAwareChunker` — tables and code blocks as atomic units
- `PropositionalChunker` — splits into atomic fact statements (Dense-X retrieval)

**Enrichment stages:**
- `ContextEnricher` — `TextEnricher(intent=ContextPrefix)` prepends document context to each chunk before embedding
- `EntityExtractor` — `TextEnricher(intent=ExtractEntities)` produces `Entity` + `Relation` records for `arcanum-graph`

**Metadata:** `TitleExtractor`, `KeywordExtractor`, `HierarchyExtractor`, `DocumentHashTracker` (SHA-256 fingerprint for incremental ingestion)

**`IngestionSourceConfig`:** admin-managed per source: source type, URI, schedule (one-time / cron / continuous), target collection, pipeline template override. Applied without restart.

### All input from document loaders is validated and sanitized before any `TextEnricher` call to prevent prompt injection via malicious file content.

---

## 10. arcanum-retrieval

### RetrievalOrchestrator

Three selectable modes:

- **Mode A — StaticRouter:** reads `RetrievalConfig.strategy_set` for the collection; routes to exactly those strategies. For latency-sensitive or resource-constrained deployments.
- **Mode B — QueryClassifier → StaticRouter:** lightweight classifier inspects query; entity mentions → adds `GraphRetriever`; analytical / "summarize across" → adds `RAPTORRetriever`; broad semantic → vector + BM25. Falls back to Mode A if classification confidence < threshold.
- **Mode C — StrategyRunner (default):** fans out to all enabled strategies in parallel; each has an independent timeout; partial results are valid — slow strategies are skipped, not waited on.

### QueryTransformer

Applied before strategies execute. Composable — multiple can be active simultaneously:

- **HyDE** — `TextEnricher` generates a hypothetical answer; both the answer and original query vectors are used
- **MultiQuery** — `TextEnricher` generates N rephrased versions; results merged before fusion
- **QueryRewrite** — `TextEnricher` rewrites for clarity and specificity

### Individual Strategies

| Strategy | Source | Method |
|---|---|---|
| `VectorRetriever` | arcanum-vector | ANN search + MetadataFilter pre-filter |
| `BM25Retriever` | arcanum-vector | Tantivy keyword search |
| `ColBERTRetriever` | arcanum-vector | Coarse ANN pass + MaxSim re-score on token vectors |
| `RAPTORRetriever` | arcanum-tree | Parallel query at leaf / mid / root levels |
| `GraphRetriever` | arcanum-graph | Entity extraction → traversal → ChunkId resolution |

### FusionEngine

- **RRF (default)** — `score = Σ 1 / (k + rank)`; k=60; robust to score scale differences
- **WeightedFusion** — configurable per-collection strategy weights
- **LearnedFusion** — small linear model trained on `arcanum-eval` feedback signals

### Reranker

`CrossEncoderReranker` (local model), `LLMReranker` (`TextEnricher(intent=Rerank)`), `ScoreFusionReranker` (fast, no model calls), `NullReranker` (passthrough)

### ResultProcessor

`Deduplicator` (cosine similarity threshold), `CitationGenerator` (document URI, title, section, chunk position, ingestion timestamp, collection ID), `QueryCache` (LRU + TTL, participates in cross-cache invalidation protocol)

---

## 11. arcanum-pipeline

DAG execution engine. Owns no stage logic — execution, ordering, parallelism, error recovery, progress tracking.

### Named Pipeline Templates

| Template | Stages | Use case |
|---|---|---|
| `StandardPipeline` | Load → Preprocess → Chunk → Embed → VectorWrite | Fast, no enrichment |
| `ContextualPipeline` | Load → Preprocess → Chunk → ContextEnrich → Embed → VectorWrite | Best single upgrade for precision |
| `GraphPipeline` | Load → Preprocess → (EntityExtract → GraphWrite ∥ Chunk → Embed → VectorWrite) | Entity-relational queries |
| `RAPTORPipeline` | Load → Preprocess → Chunk → Embed → VectorWrite + RAPTORBuild → TreeWrite | Long-document analytical queries |
| `FullPipeline` | All of the above, parallel branches | Maximum retrieval capability |

Independent DAG branches run concurrently via `tokio::join`. Users register custom pipeline DAGs via `ArcanumPipelineRegistry::register_pipeline()`.

### Error Recovery

- Core stage failure (Chunk, Embed, VectorWrite) → `Failed` → re-queued with `RetryPolicy` (exponential backoff + jitter)
- Non-core stage failure (GraphWrite, TreeWrite) → `PartialSuccess` — recorded in `IngestionReport`, does not block other branches
- Queue full → HTTP 503 + `Retry-After` header

---

## 12. arcanum-eval

First-class evaluation layer.

### Built-in Metrics

**No LLM required (ground truth labels only):**
- Hit Rate @K, MRR, NDCG @K, Precision @K, Recall @K

**Requires TextEnricher (generates + assesses answers):**
- Context Precision, Context Recall, Faithfulness, Answer Relevance

### Key Components

- **EvalRunner** — executes an evaluation suite against a collection; accepts golden dataset; returns `EvalReport`
- **BenchmarkDataset** — versioned golden datasets per collection; eval results comparable across ingestion changes
- **EvalScheduler** — optional: runs eval on schedule (e.g., after each ingestion)
- **`Evaluator` trait** — pluggable custom metrics; registered with `EvalRunner`
- **EvalReport** — per-metric scores, per-query scores, strategy breakdown (which strategies contributed to hits)

`EvalReport` is accessible via the `arcanum-engine` service layer — external agents can request a confidence baseline before using a collection.

---

## 13. arcanum-engine

Service layer. Single entry point for all business logic.

### ArcanumEngine Builder

```rust
ArcanumEngine::builder()
    .config(ArcanumConfig)
    .vector_store(impl VectorStore)    // required
    .embedder(impl Embedder)           // required
    .enricher(impl TextEnricher)       // required for Contextual/RAPTOR/Graph pipelines
    .graph_store(impl GraphStore)      // optional
    .tree_store(impl TreeStore)        // optional
    .evaluator(impl Evaluator)         // optional
    .secret_store(impl SecretStore)    // optional, defaults to env vars
    .build()
    .await?
```

`build()` validates config, checks enabled pipeline templates have required stores/providers, initializes circuit breakers, starts `IngestionWorker` pool. Fails fast at startup.

### Service Handlers

`IngestionService`, `RetrievalService`, `CollectionService`, `IngestionSourceService`, `EvalService`, `AdminService` — all business logic lives here, called identically by MCP and HTTP transports.

### Cross-cutting Concerns

- **AuthMiddleware** — validates API keys and admin JWT; enforces collection-level ACLs; shared by both transports
- **RateLimiter** — per-user, per-collection, global limits; single instance; unified across MCP and HTTP — a user connecting via both transports shares one limit pool
- **AuditLogger** — operation, user identity, collection, timestamp, result; all ingestion/retrieval/admin operations; secret values never logged
- **EventBus** — internal pub/sub; producers: IngestionWorker, circuit breakers, health checks; consumers: WebSocket handler; topics: `ingestion:progress`, `system:health`, `system:circuit-breaker`, `admin:audit`, `eval:progress`

---

## 14. arcanum-mcp

MCP is the canonical interface. All Arcanum capabilities are defined here as MCP Resources and Tools.

- **McpServer** — JSON-RPC over SSE / WebSocket; strict MCP specification compliance
- **CapabilityRegistry** — dynamic registration; Tools: `ingest`, `search`, `list_collections`, `eval_run`; Resources: `collection/{id}`, `document/{id}`, `eval/{id}`
- **SessionManager** — stateful MCP sessions; client capabilities, subscriptions, session-scoped rate limit state
- **Request handlers** — `SearchHandler`, `IngestionHandler`, `CollectionHandler`, `EvalHandler`; all delegate to `arcanum-engine` service handlers
- **StreamingHandler** — SSE streaming for long retrieval responses and ingestion progress subscriptions

---

## 15. arcanum-server

HTTP/gRPC transport for clients that cannot use MCP. Also serves the admin portal.

### Route Groups

| Route | Purpose |
|---|---|
| `/api/v1/*` | Public API — calls `arcanum-engine` service handlers directly (no MCP protocol translation) |
| `/health` | Liveness probe — always 200 if process is alive |
| `/ready` | Readiness probe — checks critical dependencies; 503 if any unavailable |
| `/admin/*` | Admin API — requires admin JWT, separate auth middleware, full audit |
| `/admin/ui/*` | Static admin frontend — embedded via `include_bytes!` |

No `McpClientAdapter`. `/api/v1/*` routes call `arcanum-engine` service handlers directly — the same handlers `arcanum-mcp` calls. No in-process protocol translation. Auth, rate limiting, and audit happen in `arcanum-engine` once, shared by both transports.

**WebSocket / SSE:** serves `EventBus` topics to browser clients; topics scoped by role (`admin:audit` requires admin JWT).

---

## 16. arcanum-admin

Team-facing portal. Served as embedded static assets. Vanilla JS + lightweight chart library — no runtime external dependencies.

### RBAC Roles

| Role | Capabilities |
|---|---|
| `admin` | Full access: manage collections, ingestion sources, retrieval settings, audit logs, API key rotation, circuit breaker reset |
| `operator` | Manage ingestion operations (start/stop/retry/cancel), view collection stats, read monitoring and logs, adjust queue capacity |
| `tester` | Manage test collections, trigger eval runs, view eval reports and retrieval quality metrics, read system health |

Admin JWT is distinct from user API keys. Admin endpoints are never exposed via the MCP interface.

### Admin API Endpoints

```
Collections          GET/POST/DELETE /admin/collections
                     GET             /admin/collections/:id/health
                     GET             /admin/collections/:id/stats

Ingestion            GET/POST        /admin/ingestion-sources
                     DELETE          /admin/ingestion-sources/:id
                     GET/POST/DELETE /admin/ingestion-operations
                     GET             /admin/ingestion-operations/:id

Retrieval            GET/POST        /admin/retrieval-config

Evaluation           GET/POST        /admin/eval/runs
                     GET             /admin/eval/runs/:id
                     GET/POST/DELETE /admin/eval/datasets

System               GET             /admin/health
                     GET             /admin/metrics
                     GET             /admin/audit-logs
                     POST            /admin/keys/rotate
                     GET/POST        /admin/circuit-breakers
                     GET/POST        /admin/cache
```

### Real-time Events

`ingestion:progress`, `system:health`, `system:circuit-breaker`, `admin:audit` (admin role only), `eval:progress`

---

## 17. Feature Flag Profiles

| Feature | Minimal | Production | Enterprise |
|---|---|---|---|
| Vector store | LanceDB (local) | pgvector | Qdrant / Weaviate |
| BM25 index | Tantivy | Tantivy | Tantivy / Elasticsearch |
| Metadata DB | SQLite | PostgreSQL | PostgreSQL (replicas) |
| Graph store | disabled | disabled | Neo4j / AGE |
| Tree store (RAPTOR) | disabled | disabled | enabled |
| Embedder | Ollama (local) | TEI (local cluster) | TEI multi-instance |
| TextEnricher | Ollama (local) | Claude Haiku / Ollama | Claude Haiku + GLiNER |
| Pipeline template | Standard | Contextual | Full |
| Retrieval mode | Mode A (vector+BM25) | Mode C (vector+BM25+ColBERT) | Mode C (all strategies) |
| Reranker | None | CrossEncoder | CrossEncoder + LLMReranker |
| Embedding cache | disabled | disabled | Redis |
| Query cache | disabled | LRU | LRU + TTL |
| Async ingestion | disabled (sync) | enabled (worker pool) | enabled (bounded queue) |
| Circuit breakers | disabled | enabled | enabled + staggered reset |
| Evaluation | disabled | disabled | arcanum-eval enabled |
| Admin portal | disabled | disabled | enabled (JWT RBAC) |
| MCP transport | disabled | enabled | enabled |
| HTTP transport | disabled | enabled | enabled |
| Redis | disabled | disabled | enabled (cache + queue) |

Each profile is a tested CI build matrix entry. Graph and Tree are Enterprise-only by default but can be enabled in Production via explicit config.

---

## 18. Security

### Authentication & Authorization

- **User API keys** — HMAC-signed, collection-scoped ACLs (allowed collections + operations + expiry)
- **Admin JWT** — RS256, short-lived (1 hour) + refresh token; three roles; completely separate from user API keys; never exposed via MCP
- **OAuth2 (optional)** — enterprise SSO; IdP roles mapped to Arcanum RBAC roles
- **Collection-level ACL** — explicit allow-list per collection; cross-collection queries require explicit multi-collection permission

### Input Validation

All document loader output validated and sanitized before entering the pipeline. Malicious file content (prompt injection via PDF text layers, JavaScript in HTML) stripped at `Preprocessor` before any `TextEnricher` call. Query inputs are length-limited and sanitized before `QueryTransformer`.

### Encryption

- **In transit** — TLS 1.3 mandatory for all external connections in Production and Enterprise; plaintext connections rejected
- **At rest** — mandatory for Production and Enterprise; optional for Minimal/Dev
  - pgvector: tablespace-level or column-level encryption
  - LanceDB: encrypted filesystem or volume (no native support)
  - Neo4j: encrypted storage configuration
  - Redis 7+: encrypted persistence

### Threat Model

| Threat | Mitigation |
|---|---|
| Prompt injection via document content | Preprocessor sanitizes before TextEnricher |
| Cross-tenant data leakage | Collection-level ACL, VectorStore namespace isolation |
| Unauthorized admin access | Admin JWT separate from user keys, IP allowlist option |
| Admin privilege escalation | RBAC in arcanum-engine, admin endpoints not in MCP |
| DoS / resource exhaustion | BoundedQueue cap (503 + Retry-After), per-user rate limiting |
| Embedding API key exposure | SecretStore trait, never logged, hot-rotatable |
| Stale retrieval results | Cross-cache invalidation protocol |
| Thundering herd on recovery | Staggered circuit breaker reset (5–15s jitter) |

---

## 19. Reliability

### Circuit Breakers

One per external dependency boundary: embedding provider, TextEnricher provider, vector store, graph store, tree store, metadata DB, Redis. Each independent.

- States: `closed` → `open` → `half-open`
- Staggered reset: randomized jitter (default 5–15s, seeded by node ID) — prevents thundering herd in multi-node deployments
- Exposed via `GET /admin/circuit-breakers`; real-time via `system:circuit-breaker` WebSocket topic

### Graceful Degradation

| Dependency unavailable | Behavior |
|---|---|
| Vector store | Ingestion queued; retrieval returns clear error (not 500) |
| Embedding provider | Ingestion queued with retry; retrieval falls back to BM25-only |
| TextEnricher | Enrichment skipped; chunk stored without prefix; recorded in IngestionReport |
| Graph store | GraphRetriever skipped in fusion; partial result returned; flagged in strategy_scores |
| Reranker | NullReranker passthrough |
| BoundedQueue full | HTTP 503 + Retry-After + descriptive error body |

### Health Probes

- `GET /health` — liveness; always 200 if process is alive
- `GET /ready` — readiness; checks all critical dependencies; 503 if any unavailable
- `GET /admin/health` — detailed dependency status for operator monitoring

---

## 20. Secrets Management

**`SecretStore` trait** abstracts all credential sources. Implementations: `EnvSecretStore`, `VaultSecretStore`, `AwsSecretsManagerStore`, `GcpSecretManagerStore`.

**Hot reload:** `SecretStore` re-fetches on configurable interval; services pick up rotated secrets without restart.

**Covered:** embedding API keys, vector store connection strings, JWT signing keys, OAuth client secrets, Redis passwords, admin JWT secrets, graph DB credentials.

**Logging guarantee:** secret values never appear in logs or audit trail. Only key path and source type are recorded.

**Key rotation:** `POST /admin/keys/rotate` — audited, hot-applied.

---

*Arcanum Architecture Specification v2.0 — produced via design review session, 2026-05-28*
