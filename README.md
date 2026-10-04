# Arcanum

**Production-grade grounded RAG engine written in Rust: retrieval, context, generation and verification, each traced to source document versions and offsets.**

Arcanum covers the whole path from raw documents to an answer you can audit:

1. **Ingest** documents through DAG pipelines with per-backend chunking, versioning and deduplication.
2. **Retrieve** with four independent strategies (dense vector, BM25 lexical, knowledge graph, and hierarchical RAPTOR tree, plus a ColBERT re-rank variant of Vector) fused into one ranking.
3. **Assemble context** that fits a token budget, with every passage numbered for citation.
4. **Generate** an answer with inline `[P1]` citations, streamed or not, from a configured LLM.
5. **Verify** that answer sentence by sentence against the passages, with evidence traced to a chunk, a document version and a byte range.

Each step is usable on its own: call `search` and bring your own LLM, call `context` and bring your own prompt, or call `generate` and `verify` for the full loop. It ships as an enterprise-ready system with pluggable backends, shadow experiment infrastructure, and a native Model Context Protocol (MCP) interface for AI assistants.

---

## Why Arcanum

Most RAG frameworks are single-strategy wrappers around one vector database. Arcanum is different:

- **Four independent retrieval strategies in a single orchestrator**: hybrid dense+sparse, graph-aware, and hierarchical retrieval are first-class, not afterthoughts.
- **Per-backend chunking**: vector, lexical (BM25), graph, and tree backends each run their own chunker and store. A knowledge graph benefits from hierarchical chunks; a vector index benefits from semantic coherence. All of them share one coordinate system (document, version, byte offsets) through the chunk registry, so results from different backends stay comparable and citable.
- **Chunk strategy experimentation built-in**: shadow experiments A/B-test a challenger chunking strategy against the live collection without affecting queries. An offline benchmark harness and an inspect API let you measure before you commit.
- **Hexagonal architecture enforced at the type level**: every storage backend, model provider, and external service is hidden behind a trait. Swap LanceDB for PgVector, Tantivy for an external search service, or Neo4j for an in-memory store with a one-line builder change and zero pipeline rewrites.
- **Built-in evidence layer**: every chunk, tree summary, graph entity, and relation can be traced back to the exact document version, byte range, and raw snapshot it came from. Document versioning and retention-based garbage collection are first-class, not bolted on.
- **Grounded answers you can check**: Context assembles numbered passages, Generate cites them inline, and Verify judges each answer sentence against those passages and reports a pass or fail with evidence offsets. The same chunk registry that powers retrieval makes every verdict traceable to source text.
- **Compiled, not interpreted**: the Rust runtime eliminates GIL contention, cold-start latency, and memory fragmentation that plague Python RAG stacks under concurrent load.
- **MCP handler included**: Claude and other AI assistants can call `search`, `ingest`, `list_collections`, `eval_run`, `get_context`, `generate`, and `verify` over JSON-RPC 2.0 directly; all seven tools are implemented (see [MCP Integration](#mcp-integration)).
- **Three runtime modes**: `Development` (SQLite, in-memory stores permitted), `Production`, and `Enterprise` (both require Postgres + LanceDB/Neo4j). Startup validation enforces the SQLite-vs-Postgres split only; RBAC, audit logging, and secret-store rotation are available in every mode, not gated by `runtime_mode` (see [Runtime Modes](#runtime-modes)).

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
│   ┌─────────────────┐  ┌──────────────────┐  ┌──────────────────────┐  │
│   │ ContextService  │  │ GenerateService  │  │    VerifyService     │  │
│   │ (pack + number) │  │ (cite answers)   │  │ (judge, trace)       │  │
│   └─────────────────┘  └──────────────────┘  └──────────────────────┘  │
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

The 19-crate workspace maps cleanly to layers:

| Layer | Crates |
|---|---|
| **Domain core** | `arcanum-core`, `arcanum-engine` |
| **Ingestion** | `arcanum-ingestion`, `arcanum-pipeline` |
| **Evidence & provenance** | `arcanum-evidence` |
| **Retrieval** | `arcanum-retrieval`, `arcanum-eval` |
| **Grounded answers** (pure logic, no I/O) | `arcanum-context`, `arcanum-generate`, `arcanum-verify` |
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
Extracts entity names from the query, traverses a knowledge graph (Neo4j or in-memory) up to a configurable hop depth, and returns the chunks that mention the matched entities, ranked by hop distance (nearer entities score higher). Effective for relationship-heavy domains.

### 4 — RAPTOR (Hierarchical Tree)
Builds a recursive summarisation tree over ingested chunks using K-means clustering. At query time, traversal spans all levels (coarse-to-fine), with level-weighted cosine scoring. Handles abstractive questions that require document-level reasoning, not just chunk-level matches.

### ColBERT (Re-Rank Variant of Vector)
A variant of Vector rather than a separate backend. Performs a coarse ANN pass followed by a MaxSim token-vector re-rank. Falls back gracefully to coarse scores when token vectors are absent. Provides the precision of cross-encoder models at closer-to-bi-encoder latency.

### Orchestration Modes

| Mode | Behaviour |
|---|---|
| `Static` | Fixed retriever order, first result set returned |
| `QueryClassified` | Classifier routes queries to the most relevant retriever |
| `ParallelFusion` | All retrievers run concurrently; document-level RRF fusion |

**Chunk registry:** every backend registers its chunks in the chunk registry (`ChunkMetadataStore`) with the backend name, the document, the version and UTF-8 byte offsets into the preprocessed document. BM25, Graph and RAPTOR hydrate their hits from the registry, so each hit points at citable source text. A registry is required whenever lexical, graph or tree is enabled; the engine fails to start otherwise.

**Fusion key:** Arcanum keys cross-backend fusion on `document_id`, not `chunk_id`. With per-backend chunkers each backend produces independent `ChunkId`s that never align; `document_id` is the stable, correct cross-backend key. A document appearing in both vector and graph results is boosted; one appearing in only one is not penalised.

---

## Ingestion Pipelines

Pipelines are DAGs of typed stages. The stage runner is async and supports concurrent stages with explicit dependency declarations.

### Document Preprocessing

Before chunking, every document passes through a preprocessor chain that converts raw bytes to clean text. Arcanum ships two preprocessor sets:

| Preprocessor set | Covered MIME types | When used |
|---|---|---|
| `default_chains` (built-in) | PDF, HTML, XHTML, EPUB, DOCX | No `[ingestion.docling]` config |
| `DoclingPreprocessor` (docling) | PDF, DOCX, PPTX, XLSX, EPUB, HTML, XHTML, PNG, JPEG, TIFF | `[ingestion.docling]` present in config |

`DoclingPreprocessor` integrates with [docling-serve](https://github.com/DS4SD/docling-serve) and extends format coverage to presentations, spreadsheets, and images, none of which the built-in parsers handle.

**HTTP backend**: posts each document to a running docling-serve instance. Supports synchronous and asynchronous (poll-based) modes:

```toml
[ingestion.docling.backend]
type             = "http"
base_url         = "http://docling-serve:5001"
timeout_secs     = 300
use_async        = true   # poll for completion instead of blocking
poll_interval_ms = 2000
```

**CLI backend**: shells out to a local `docling` binary. Useful for air-gapped environments or local development without a server:

```toml
[ingestion.docling.backend]
type    = "cli"
command = "docling"
```

When `[ingestion.docling]` is absent, the engine falls back to `default_chains`, which covers the five most common formats without any external dependency.

### Per-Backend Chunking

Every pipeline template runs independent chunking branches from the same preprocessed document:

```
Preprocess ─┬─→ vector_chunk (chunkers.vector) → embed → vector_write
            ├─→ lexical_chunk (chunkers.lexical) → lexical_write
            ├─→ graph_chunk  (chunkers.graph)  → entity_extract → graph_write
            └─→ tree_chunk   (chunkers.tree)   → tree_embed → raptor_build → tree_write
```

Each branch uses its own `Arc<dyn Chunker>` resolved from a two-tier config:

1. **Collection-level override**: per-collection `PerBackendChunkConfig`, set at creation or updated via the collection API.
2. **Global default**: `IngestionConfig.chunking`, applied when the collection has no override.

Both paths go through `ChunkRegistry.build()` at job-start time, so a bad config fails immediately, not mid-ingest.

### Built-in Chunking Strategies

| Strategy | Key parameter | Best for |
|---|---|---|
| `fixed` | `chunk_size`, `overlap` | Predictable token budgets |
| `semantic` | `max_chars` | Sentence-boundary-aware splitting |
| `propositional` | — | Claim-level granularity |
| `hierarchical` | — | Knowledge graph construction |
| `structure` | `max_chunk_chars` | Markdown / HTML with heading structure |

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

Stages are opt-in based on what is present on the engine. If no `graph_store` is wired, graph stages are silently skipped. No code changes required; just wire or omit a dependency in the builder.

### Document Deduplication

A `DocumentRegistry` tracks each `(source_uri, collection_id)` pair. On re-ingest, the pipeline compares content hashes and skips unchanged documents, replaces changed ones (cleaning stale chunks first), and handles interrupted-cleanup recovery via a `Replacing` status.

---

## Evidence & Provenance

Every retrievable unit (a vector chunk, a RAPTOR tree summary, a graph entity, a graph relation) can be traced back to the exact document version and byte range it came from. This answers "where did this come from?" for compliance, debugging, and citation use cases.

### Document Versioning

`DocumentVersionStore` tracks every ingested version of a `(source_uri, collection_id)` pair, gated by a per-collection `VersioningPolicy`:

| Policy | Behaviour |
|---|---|
| `Replace` (default) | Re-ingesting supersedes the prior version; only the latest is queryable |
| `AppendOnly` | All versions remain `Active` indefinitely |
| `RetentionBased { days }` | Superseded versions are kept for `days`, then garbage-collected |

`SnapshotStore` persists the raw bytes (and an optional canonical JSON sidecar) for each version, so the original document can always be re-fetched, not just the chunks derived from it. `ChunkMetadataStore` records, per chunk, the `document_id`, `version_num`, `source_uri`, `snapshot_uri`, `page`/`section`/`block_ids`, and exact `offset_start`/`offset_end` in the source document, written alongside the vector store during ingestion.

### Resolving Evidence

`EvidenceResolver` (implemented by `DefaultEvidenceResolver` in `arcanum-evidence`) turns an ID into a `ProofChain`:

```rust
pub struct ProofChain {
    pub root:        ProofNode,        // the resolved unit, with nested children
    pub raw_sources: Vec<RawSourceRef>, // every underlying document span cited
}
```

- `resolve_chunk(chunk_id)`: looks up `ChunkMetadataRecord`, cross-checks the version is still `Active` (flagged in the response if not), and returns a single-source `ProofChain`.
- `resolve_tree_node(node_id)`: fans out to every leaf chunk under a RAPTOR summary node.
- `resolve_entity(entity_id)` / `resolve_relation(source, type, target)`: fans out to every chunk a graph entity or relation was extracted from.

```http
GET /evidence/chunk/{chunk_id}
GET /evidence/tree-node/{node_id}
GET /evidence/entity/{entity_id}
GET /evidence/relation/{source_id}/{relation_type}/{target_id}
```

All four require a Bearer token and return `404` if the ID is unresolvable, `503` if no resolver is wired.

### Retention & Garbage Collection

For collections using `RetentionBased` policy, `GcWorker` (implemented by `PostgresGcWorker`) purges superseded versions once their retention window expires, removing the snapshot, vector chunks, tree nodes, graph entities, and chunk metadata for that specific version, without touching any other version that happens to share the same `source_uri`.

```http
POST /admin/gc
Authorization: Bearer <admin-token>
```

Returns a `GcReport` (`versions_deleted`, `snapshots_removed`, `chunks_removed`, `errors`). Run it on a schedule (cron, k8s CronJob) for collections under retention policy.

### Wiring

```rust
let engine = ArcanumEngine::builder()
    // ...
    .version_store(version_store)              // SqliteDocumentVersionStore (dev) / PostgresDocumentVersionStore (prod)
    .snapshot_store(snapshot_store)             // LocalSnapshotStore / S3-backed
    .chunk_metadata_store(chunk_metadata_store) // InMemoryChunkMetadataStore (dev) / PostgresChunkMetadataStore (prod)
    .evidence(Arc::new(DefaultEvidenceResolver::new(
        chunk_metadata_store.clone(), version_store.clone(), tree_store.clone(), graph_store.clone(),
    )))
    .gc_worker(gc_worker) // PostgresGcWorker: requires Postgres, production only
    .build()
    .await?;
```

`chunk_metadata_store`, `evidence`, and `gc_worker` are all optional: omit them and the `/evidence/*` routes return `503` and `/admin/gc` does too, with no other behaviour change. `GcWorker` requires Postgres for its bookkeeping (`document_versions` table), so there is no in-memory equivalent for local development.

---

## Context API

`POST /api/v1/context` (and the MCP `get_context` tool) turns a query or a conversation into prompt-ready context for your own LLM: passages that fit a token budget, each tagged with a citation id you can map back to a document version and byte range. This endpoint does not call a generator; you place the context in your prompt. To have Arcanum call one for you, see the [Generate API](#generate-api).

Context needs the chunk registry (`storage.database_url`, or a `ChunkMetadataStore` supplied to the builder). Without one the endpoint returns `503`.

### Request

```bash
curl -X POST /api/v1/context \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "collection_id": "legal",
    "query": "What is the notice period for termination?",
    "token_budget": 3000,
    "render": "xml"
  }'
```

| Field | Default | Meaning |
|---|---|---|
| `collection_id` | required | Collection to search; the caller must have access |
| `query` | | A single question. Exactly one of `query` or `messages` is required |
| `messages` | | Conversation as `[{"role": "user" \| "assistant", "content": "..."}]`; the last message must be from the user |
| `token_budget` | `context.default_token_budget` (4000) | Upper bound on the tokens of the assembled context, minimum 200 |
| `background_share` | `0.2` | Fraction of the budget reserved for background summaries, 0.0 to 1.0 |
| `candidate_k` | `context.default_candidate_k` (50) | Candidates fetched per strategy, 1 to 200 |
| `render` | none (MCP: `xml`) | `numbered`, `xml`, or `markdown`; omit to receive structured data only |

With `messages`, a configured enricher rewrites the conversation into one standalone question. If there is no enricher, the rewrite fails, or the result is empty or over 1000 characters, Arcanum uses the last user message and reports `resolved_query_source: "fallback"`.

### Response

```json
{
  "resolved_query": "What is the notice period for termination?",
  "resolved_query_source": "original",
  "passages": [{
    "ref_id": "P1",
    "document_id": "...", "version_num": 2,
    "source_uri": "file:///contracts/msa.pdf",
    "snapshot_uri": "...", "canonical_uri": null,
    "section": "14. Termination", "page": 9,
    "offset_start": 40213, "offset_end": 40871,
    "text": "Either party may terminate on 60 days' written notice ...",
    "chunk_ids": ["..."], "strategies": ["vector", "bm25"],
    "score": 0.031
  }],
  "background": [{ "ref_id": "S1", "text": "...", "level": 1, "document_id": "...", "covers": ["..."], "score": 0.016 }],
  "usage": { "budget": 3000, "used": 912, "passages": 640, "background": 210, "dropped_passages": 3, "counter": "approx_cl100k" },
  "retrieval": { "queries": ["..."], "strategies_ok": ["vector", "bm25"], "strategies_failed": [] },
  "rendered": "<documents>...</documents>"
}
```

- Passages are exact source slices (re-read from the chunk registry, without any enrichment prefix). Hits from different strategies that cover the same text are clustered, scored with reciprocal rank fusion, and packed best-first until the budget is spent. Overlapping or touching passages from one document are merged, then ordered by document and offset.
- `ref_id` values (`P1`, `P2`, ... for passages and `S1`, `S2`, ... for RAPTOR background summaries) are stable within one response and appear in the rendered text, so a model can cite them and you can resolve them to `source_uri`, `version_num`, and the byte range.
- `usage.used` counts the rendered output when `render` is set, using an approximate cl100k counter with a 10% safety margin. Treat it as an estimate for other tokenizers.
- A strategy that fails or times out is listed in `retrieval.strategies_failed` and does not fail the request. The call returns `503` only if every strategy fails.
- Errors: `400` invalid request, `403` no access to the collection, `503` circuit open, all strategies failed, or no chunk registry.

### Render formats

| `render` | Shape |
|---|---|
| `numbered` | `[P1] source_uri (v2)` header line, then the passage text; a `Background:` block with `[S1] ...` lines |
| `xml` | `<documents><document source="..." version="2"><passage ref="P1">...</passage></document></documents>`, then `<background><summary ref="S1">...</summary></background>`; text is XML-escaped |
| `markdown` | `### source_uri (v2)` per document with `[P1] text` paragraphs; a `#### Background` section |

### Use it with your own LLM

```python
ctx = requests.post(f"{BASE}/api/v1/context", headers=auth, json={
    "collection_id": "legal",
    "messages": history,          # the conversation so far, last turn from the user
    "token_budget": 3000,
    "render": "xml",
}).json()

prompt = (
    "Answer using only the context below and cite passages by ref, e.g. [P1].\n\n"
    f"{ctx['rendered']}\n\nQuestion: {history[-1]['content']}"
)
answer = my_llm(prompt)
# Map [P1] in the answer back to ctx["passages"][0]["source_uri"] and its offsets.
```

Pick `token_budget` as the room your prompt has left after the system prompt, the conversation, and the answer you want back.

### Configuration

```toml
[context]
default_token_budget = 4000   # used when a request omits token_budget
default_candidate_k  = 50     # candidates per strategy when a request omits candidate_k
rewrite_max_messages = 6      # most recent messages sent to the query rewriter

[enrichment]
rewrite_query_provider = "ollama-small"   # optional: route rewriting to a named enricher
```

---

## Generate API

`POST /api/v1/generate` (and the MCP `generate` tool) answers a question or summarizes a topic from your collection with a built-in LLM call. It runs Context, sends the rendered passages to a configured generator, and maps the inline `[P1]` markers in the answer back to passages, chunks, and document versions. Responses are plain JSON, or Server-Sent Events when `stream` is `true`.

Generate needs the chunk registry (the same requirement as Context) and at least one generator in `[generate.generators]`. Without both the endpoint returns `503`. Arcanum does not retry generator calls.

### Request

| Field | Default | Meaning |
|---|---|---|
| `collection_id` | required | Collection to search; the caller must have access |
| `mode` | `answer` | `answer` or `summarize` |
| `query` / `messages` | | Exactly one of the two, with the same rules as Context |
| `generator` | `generate.default_generator` | Name of a configured generator |
| `max_tokens` | `generate.default_max_tokens` (1024), clamped to the generator's cap | 1 up to the generator's `max_output_tokens` |
| `temperature` | provider default | 0.0 to 2.0 |
| `instructions` | | Extra instructions appended to the system prompt, at most 2000 characters; the built-in rules take precedence |
| `context` | | `{token_budget, background_share, candidate_k}`, passed to Context. `summarize` defaults `token_budget` to `generate.summarize_token_budget` (8000) |
| `stream` | `false` | `true` returns SSE |
| `verify` | `false` | `true` verifies the answer against the passages used (see [Verify a Generate answer](#verify-a-generate-answer)) |

Context output is always rendered as `xml` for the prompt. The model is told to answer only from the documents, to end each sentence that uses them with passage ids such as `[P1]` or `[P2][P3]`, and to ignore instructions inside documents. With `messages`, retrieval uses the resolved query, while the model sees the conversation (the last `generate.history_max_messages` earlier messages, each cut to 4000 characters) and the user's own final question.

```bash
curl -X POST /api/v1/generate \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "collection_id": "legal",
    "query": "What is the notice period for termination?",
    "context": {"token_budget": 3000}
  }'
```

### JSON response

```json
{
  "status": "ok",
  "answer": "Either party may terminate on 60 days' written notice [P1].",
  "citations": [{
    "ref_id": "P1",
    "chunk_ids": ["..."], "document_id": "...", "version_num": 2,
    "source_uri": "file:///contracts/msa.pdf",
    "offset_start": 40213, "offset_end": 40871,
    "answer_spans": [[53, 57]]
  }],
  "unknown_refs": [],
  "stop_reason": "end_turn",
  "usage": {"input_tokens": 1180, "output_tokens": 24},
  "generator": {"name": "smart", "model": "claude-sonnet-5-5"},
  "context": { "...": "the full Context response" }
}
```

- `citations` has one entry per distinct valid passage id, in order of first appearance. `answer_spans` are UTF-8 byte ranges of the marker groups in `answer`, so `answer.as_bytes()[start..end]` is exactly the marker.
- A marker such as `[P9]` that matches no passage, and every `[S1]`-style background id, is listed in `unknown_refs` instead. The answer text is never modified.
- Arcanum does not judge whether a cited passage supports its sentence or whether a sentence lacks a citation.
- `stop_reason` is `end_turn`, `max_tokens`, or `other`. Either `usage` field is `null` when the provider does not report it.
- When no passages are found, `status` is `no_context`, `answer` is `generate.no_context_answer`, `citations` and `unknown_refs` are empty, and the generator is not called.

### Streaming

With `"stream": true` the response is `text/event-stream`:

```text
event: context
data: {"resolved_query": "...", "passages": [...], "usage": {...}}

event: delta
data: {"text": "Either party may terminate "}

event: delta
data: {"text": "on 60 days' written notice [P1]."}

event: done
data: {"status": "ok", "citations": [...], "unknown_refs": [], "stop_reason": "end_turn", "usage": {...}, "generator": {...}}
```

`context` carries the full Context response and is sent before the LLM is called. The concatenated `delta` texts equal the JSON `answer`, and `done` carries the JSON response's fields other than `answer` and `context`. A failure after the stream starts arrives as `event: error` with `{"error": "..."}`, after which the connection closes; treat the text received so far as incomplete. If the client disconnects, the upstream LLM request is cancelled.

### Errors

| Status | Cause |
|---|---|
| `400` | Invalid request: any Context rule, an unknown `mode` or `generator`, `max_tokens` of 0 or above the generator's cap, `temperature` out of range, or `instructions` too long |
| `403` | No access to the collection |
| `502` | The generator returned an error or its stream ended early; the body is always `{"error": "generation failed"}` and the detail is logged server-side |
| `503` | No chunk registry or no generator, all retrieval strategies failed, or the generator's circuit breaker is open |
| `504` | No first token within `first_token_timeout_secs`, or the generation exceeded `total_timeout_secs` |

`no_context` is served with `200` even when the generator's circuit is open. For SSE, `400`, `403`, and `503` are returned before the stream starts; `502` and `504` arrive as `error` events.

### Configuration

```toml
[generate]
default_generator        = "smart"
default_max_tokens       = 1024
summarize_token_budget   = 8000
history_max_messages     = 10
first_token_timeout_secs = 30
total_timeout_secs       = 120
no_context_answer        = "No relevant information was found in the collection."

[generate.generators.smart]
protocol          = "anthropic"            # anthropic | openai_compatible
model             = "claude-sonnet-5-5"
api_key_env       = "ANTHROPIC_API_KEY"    # name of the env var holding the key
max_output_tokens = 4096

[generate.generators.local]
protocol          = "openai_compatible"
base_url          = "http://localhost:11434/v1"
model             = "llama3.1"
max_output_tokens = 2048
```

Engine build fails with a config error when generators exist but `default_generator` is unset or names no generator, or when a configured `api_key_env` variable is unset. `ArcanumEngineBuilder::generator(name, generator, max_output_tokens)` registers or replaces a generator in code. Metrics: `arcanum_generation_total{generator, mode, status}` (`ok`, `no_context`, `error`, `timeout`), `arcanum_generation_duration_seconds{generator}`, and `arcanum_generation_tokens_total{generator, kind}`.

---

## Verify API

`POST /api/v1/verify` (and the MCP `verify` tool) checks an answer sentence by sentence against the passages it was generated from. Each sentence gets a verdict, and every supporting claim is tied to evidence in the stored source: a chunk, a document version, and a byte range. It works on answers from Generate and on answers from your own LLM, as long as you used Context's passages.

Verify needs a judge, the name of a generator from `[generate.generators]` set as `[verify].judge`, and the chunk registry. Without both the endpoint returns `503` (`verification requires a configured judge and a chunk registry`). The judge shares that generator's HTTP client and circuit breaker, so Generate and Verify trip together when one upstream is down. Passage text is always re-read from the registry by `chunk_ids`; text you send is never trusted. Verification costs at least one extra LLM call per answer.

### Request

```bash
curl -X POST /api/v1/verify \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "collection_id": "docs",
    "answer": "Acme was founded in 1998 [P1]. Its headquarters are in Berlin [P2].",
    "passages": [
      {"ref_id": "P1", "chunk_ids": ["..."]},
      {"ref_id": "P2", "chunk_ids": ["...", "..."]}
    ]
  }'
```

| Field | Default | Meaning |
|---|---|---|
| `collection_id` | required | Collection the passages belong to; the caller must have access |
| `answer` | required | The answer text, at most `verify.max_answer_chars` characters |
| `passages` | required | `[{ref_id, chunk_ids}]`, at most `verify.max_passages`. Extra fields are ignored, so Context's `passages` array can be forwarded unchanged |
| `judge` | `verify.judge` | Name of a configured generator to use as the judge |
| `strict_citations` | `false` | Also fail the overall verdict on `miscited` and `uncited_supported` sentences |

### Response

```json
{
  "verdict": "fail",
  "strict_citations": false,
  "counts": {"supported": 1, "miscited": 0, "uncited_supported": 0,
             "partial": 0, "unsupported": 1, "no_claim": 0},
  "sentences": [{
    "span": [0, 30],
    "text": "Acme was founded in 1998 [P1].",
    "verdict": "supported",
    "cited": ["P1"],
    "invalid_refs": [],
    "claims": [{
      "text": "Acme was founded in 1998", "supported": true,
      "evidence": [{
        "ref_id": "P1", "chunk_id": "...", "document_id": "...",
        "version_num": 3, "version_status": "active", "source_uri": "...",
        "offset_start": 1204, "offset_end": 1219,
        "quote": "founded in 1998", "quote_matched": true
      }]
    }]
  }],
  "passages_unavailable": [],
  "judge": {"name": "cheap", "model": "..."},
  "usage": {"input_tokens": 1830, "output_tokens": 412, "judge_calls": 1}
}
```

- `span` is a UTF-8 byte range into `answer`, and `text` is `answer[span]`.
- `cited` lists the passage ids the sentence cites that name an available passage. Any other id (unknown, unavailable, or an `[S1]`-style background id) goes to `invalid_refs`.
- `evidence` lists every passage that supports a claim, wherever the sentence cites. The judge copies the shortest verbatim quote; when Verify finds it in the stored text (exact, then whitespace-collapsed), `offset_start` and `offset_end` are the quote's byte range in the source, `chunk_id` is the chunk containing it, and `quote_matched` is `true`. Otherwise the range is the whole passage, `chunk_id` is the passage's first chunk, and `quote_matched` is `false`, which flags a paraphrased or invented quote.
- `version_status` is `active`, `superseded`, `deleted`, or `unknown`. It is reported, not enforced.
- `passages_unavailable` lists passages with a missing chunk (for example garbage-collected). They are not sent to the judge and cannot support a claim. When none is available the judge is not called, and every sentence except code blocks is `unsupported`.
- `usage` sums all judge calls, including retries; a token count is `null` when any call did not report it.

### Verdicts

The judge reports facts and Verify computes the verdicts, in this order:

| Verdict | Condition |
|---|---|
| `no_claim` | The sentence makes no factual claim (greetings, hedges, "nothing found"), or is a fenced code block |
| `unsupported` | No claim of the sentence has a supporting passage |
| `partial` | Some claims are supported and some are not; the unsupported claims have `supported: false` |
| `supported` | Every claim is supported by a passage the sentence cites |
| `uncited_supported` | Every claim is supported, but the sentence cites nothing valid |
| `miscited` | Every claim is supported and the sentence cites something, but some claim has no supporting passage among its citations |

The overall `verdict` is `fail` if any sentence is `partial` or `unsupported`, otherwise `pass`. With `strict_citations: true` it also fails on `miscited` and `uncited_supported`. An answer whose sentences are all `no_claim` passes. Sentences are split with Unicode sentence boundaries and at every newline, and a citation marker group right after a sentence attaches to it, so `foo. [P1]` and `foo [P1].` behave the same.

### Errors

| Status | Cause |
|---|---|
| `400` | Invalid request (empty answer, over the length or passage limits, a `ref_id` not matching `^P\d{1,3}$`, a duplicate `ref_id`, a passage without `chunk_ids`, an unknown `judge`), a chunk from another collection, a passage spanning several document versions, passages that alone exceed the judge input budget, or an answer that needs more than 25 judge batches (`answer needs too many judge batches`) |
| `403` | No access to the collection |
| `502` | The judge failed upstream, or returned invalid output twice; the detail is logged, not returned |
| `503` | Verify not configured, no chunk registry, or the judge's circuit breaker is open |
| `504` | A judge call exceeded `judge_timeout_secs` |
| `500` | Registry or version store failure |

A judge reply that fails validation (bad JSON, a missing or unknown sentence id, a `claim` without claims, a `ref` outside the request's passages, or truncation at `max_tokens`) is retried once with the validation error appended. Any batch that still fails fails the whole request; no partial result is returned. Long answers are split into batches that run up to four at a time.

### Verify a Generate answer

Set `"verify": true` on a Generate request (REST or the MCP `generate` tool) to verify the answer against the passages Generate used, with the default judge and non-strict citations. If Verify is not configured the request fails with `503` before Context or the generator is called. A verification failure never fails the generation; the outcome is in `verification`:

```json
"verification": {"status": "ok", "verdict": "pass", "counts": {"...": 0}, "sentences": ["..."]}
"verification": {"status": "error", "code": "judge_timeout", "message": "judge timed out"}
```

An `ok` verification carries all the fields of the Verify response. Error codes are `judge_unavailable` (breaker open), `judge_upstream`, `judge_timeout`, `judge_invalid_output`, `invalid` (for example a chunk garbage-collected between generation and verification), and `internal`. `verification` is `null` for a `no_context` answer (no judge call is made), and a failed generation is not verified. An answer cut off at `max_tokens` is verified as is. A Generate call whose context exceeds Verify's limits (more than `verify.max_passages` passages, or passages over the judge input budget) returns `verification: {status: "error", code: "invalid"}` while the answer is still delivered. Without `verify: true`, the response and the stream are unchanged.

With `stream: true` the event order is `context`, `delta`..., `done`, then `verification`. A verification error arrives in the `verification` event, not as `error`, because generation succeeded:

```text
event: verification
data: {"status": "ok", "verdict": "pass", ...}
```

**Gating caveat.** In SSE mode the answer text reaches the client before the verdict. A caller that gates on the verdict must buffer the text until `verification` arrives, or use JSON mode. Treat `status: "error"` as a failure.

### Configuration

```toml
[verify]
judge = "cheap"                 # a name from [generate.generators.*]; absent = Verify disabled
max_answer_chars = 20000
max_passages = 50
max_judge_input_tokens = 24000
max_sentences_per_batch = 40
judge_max_output_tokens = 8192
judge_timeout_secs = 90
```

Engine build fails with a config error when `judge` names no configured generator. `judge_timeout_secs` bounds each judge call including draining its stream, and each retry gets a fresh timeout. Upstream errors and timeouts count as circuit breaker failures; invalid judge output does not. The audit log records one `verify` entry per request (collection, judge, overall verdict, counts, judge calls) and never the answer text. Metrics: `arcanum_verify_requests_total{outcome}` (`pass`, `fail`, or an error code), `arcanum_verify_sentences_total{verdict}`, `arcanum_verify_judge_calls_total{result}`, and `arcanum_verify_duration_seconds`.

Judge quality bounds verdict quality: a weak judge can mark support that is not there, which `quote_matched: false` helps surface. Sentence splitting is heuristic (an abbreviation such as "e.g." can split a sentence). Summarize-mode answers lean on RAPTOR background summaries, which are not evidence, so they may verify as `unsupported` more often.

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

Returns `recall_at_5`, `recall_at_10`, `mean_chunk_tokens`, `chunk_size_p50`, `chunk_size_p95` per strategy. No LLM calls; recall against labeled document IDs is the signal.

### C — Shadow Experiments (live A/B testing)

Test a challenger strategy on real traffic without affecting queries:

```http
# Start experiment: all new documents are also written to a shadow namespace
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

Shadow writes are best-effort: a shadow write failure never fails the primary ingestion job. Only the primary namespace is queried. At most one `Active` experiment per collection at a time.

---

## Getting Started

```rust
use arcanum_engine::ArcanumEngine;
use arcanum_core::config::ArcanumConfig;
use std::sync::Arc;

// Minimal: vector search only
let engine = ArcanumEngine::builder()
    .auth_secret("your-32-char-minimum-secret-here")
    .vector_store(Arc::new(my_lance_db_store))
    .embedder(Arc::new(my_ollama_embedder))
    .build()
    .await?;

// Full: all retrieval strategies
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

// Via HTTP: URI reference
curl -X POST /api/v1/ingest \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"source_uri":"s3://bucket/doc.pdf","collection_id":"legal","pipeline":"full"}'

// Via HTTP: direct upload
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

### Ask, then check the answer

With a chunk registry, a generator and a judge configured (see [Generate API](#generate-api) and [Verify API](#verify-api)), one request retrieves, answers with citations and verifies the answer:

```bash
curl -X POST http://localhost:8080/api/v1/generate \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"collection_id":"legal","query":"What counts as a material breach?","verify":true}'
# -> { "answer": "... [P1] ...", "citations": [...], "verification": { "status": "ok", "verdict": "pass", ... } }
```

The same pieces are available separately: `/api/v1/context` returns packed, numbered passages for your own prompt, and `/api/v1/verify` checks any answer against passages you supply.

---

## Enterprise Features

### Authentication & Authorisation

Two token types, one middleware:

- **HMAC API keys** (`HS256`): issued per user with an `allowed_collections` scope list. `is_admin: true` grants full access.
- **RS256 admin JWTs**: for admin operations; validated against a configurable public key PEM.

All MCP tool calls and admin routes require an `Authorization: Bearer <token>` header. The MCP server performs per-request token extraction; there are no session tokens or shared credentials.

### Role-Based Access Control (RBAC)

Admin operations are gated by a three-tier role hierarchy:

| Role | Capabilities |
|---|---|
| `Tester` | Read health, metrics |
| `Operator` | + Collection management, audit log access, ingestion sources, chunk experiments |
| `Admin` | + Key rotation, all destructive operations |

### Audit Logging

Every authenticated operation is recorded with `user_id`, `collection_id`, operation type, result status, and timestamp. The audit trail is queryable via the admin API. `audit_retention_days` (default 90) exists in config but isn't enforced yet: `AuditLogger` is an unbounded in-memory `Vec` with no eviction.

### Circuit Breakers

Independent circuit breakers protect the embedding provider and the vector store. When the failure threshold is exceeded, the breaker opens and requests fail fast with a clear error rather than queuing behind a degraded dependency. Shadow writes respect the vector store circuit breaker and skip rather than block.

### Secret Store & Hot Reload

`SecretStore` is a trait: back it with HashiCorp Vault, AWS Secrets Manager, or environment variables. `ArcanumEngine` holds the store and spawns a background task that calls `store.reload()` on a configurable interval (default 300 s). `POST /admin/rotate-keys` triggers an immediate reload after key rotation.

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
| `arcanum_generation_total` | Counter | Generate calls by generator, mode and status |
| `arcanum_verify_requests_total` | Counter | Verify calls by outcome (`pass`, `fail`, or an error code) |
| `arcanum_verify_judge_calls_total` | Counter | Judge calls by result, for judge health |

---

## Runtime Modes

```toml
[global]
runtime_mode = "enterprise"   # development | production | enterprise
```

| Mode | Metadata backend | Startup enforcement |
|---|---|---|
| `development` | SQLite permitted | None (suitable for local iteration) |
| `production` | Postgres required | SQLite rejected at startup |
| `enterprise` | Postgres required | SQLite rejected at startup (identical to `production`; no additional checks yet) |

Mode is also readable from the `ARCANUM_RUNTIME_MODE` environment variable. Config is layered: defaults → file (`config.toml` or `config.yaml`) → environment variables, with later layers taking precedence.

`production` and `enterprise` are functionally identical today beyond the SQLite rejection above. RBAC (see [Role-Based Access Control](#role-based-access-control-rbac)) and admin-JWT validation work the same in every mode; they are not gated by `runtime_mode`. The `ip_allowlist` config field is likewise present but not yet enforced by any request path.

---

## MCP Integration

Arcanum ships an MCP JSON-RPC 2.0 handler (`arcanum-mcp`) plus a minimal standalone `arcanum-mcp` binary. The bin is env-driven, configured via `ARCANUM_AUTH_SECRET` (required), `MCP_PORT` (default `8081`), and `ARCANUM_DB_PATH` (SQLite version store), but wires no embedder or vector store, so `search`/`ingest` need library wiring (see `examples/`) before they return real results. Mounting the handler behind your own `main.rs` alongside your HTTP server remains the full-integration path (see [DEVELOPMENT.md](DEVELOPMENT.md#15-mcp-integration)):

| Tool | Parameters | Status |
|---|---|---|
| `search` | `query`, `collection_id`, `top_k` | Implemented; returns an array of scored chunks |
| `ingest` | `source_uri`, `collection_id`, `pipeline` | Implemented; returns an `operation_id` for tracking |
| `list_collections` | — | Implemented; returns collections visible to the caller, ACL-filtered |
| `eval_run` | `collection_id` | Implemented; params: `collection_id`, `samples[{query, relevant_chunk_ids}]`, `k` (default 5, max 100) |
| `get_context` | `collection_id`, `query` or `messages`, optional `token_budget`, `background_share`, `candidate_k`, `render` | Implemented; returns the rendered context (default `xml`) as text plus the full response as `structuredContent`; needs a chunk registry (see [Context API](#context-api)) |
| `generate` | `collection_id`, `query` or `messages`, optional `mode`, `generator`, `max_tokens`, `temperature`, `instructions`, `context`, `verify` | Implemented; returns the answer as text plus the full JSON response as `structuredContent`; always non-streaming; needs a chunk registry and a generator (see [Generate API](#generate-api)) |
| `verify` | `collection_id`, `answer`, `passages[{ref_id, chunk_ids}]`, optional `judge`, `strict_citations` | Implemented; returns the response as JSON text plus `structuredContent`; needs a judge and a chunk registry (see [Verify API](#verify-api)) |

Every tool call requires a valid Bearer token. The MCP server validates the token against `engine.auth` on each request: no shared session, no bypass.

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

# Docling preprocessor: omit this section to use the built-in parsers
[ingestion.docling.backend]
type             = "http"
base_url         = "http://docling-serve:5001"
timeout_secs     = 300
use_async        = false

# Global default chunker: per-collection overrides take precedence
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

[context]
default_token_budget = 4000
default_candidate_k  = 50
rewrite_max_messages = 6

[generate]
default_generator = "smart"

[generate.generators.smart]
protocol          = "anthropic"
model             = "claude-sonnet-5-5"
api_key_env       = "ANTHROPIC_API_KEY"
max_output_tokens = 4096

[verify]
judge = "cheap"   # a name from [generate.generators.*]; absent = Verify disabled

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
| `arcanum-models` | HTTP embedding clients (Ollama, OpenAI), Redis cache, streaming Anthropic and OpenAI-compatible generators |
| `arcanum-ingestion` | Loaders, preprocessors (HTML/PDF/EPUB/DOCX + DoclingPreprocessor for PPTX/XLSX/images), chunkers, ChunkRegistry |
| `arcanum-middleware` | Circuit breaker, retry policy, bounded queue |
| `arcanum-pipeline` | DAG stage runner and built-in pipeline templates |
| `arcanum-evidence` | `DefaultEvidenceResolver`: resolves chunks/tree nodes/entities/relations back to source documents |
| `arcanum-retrieval` | Multi-strategy orchestrator and all Retriever impls |
| `arcanum-context` | Context API packing: span-level clustering, token-budgeted selection, numbered/xml/markdown rendering, conversation rewriting |
| `arcanum-generate` | Generate API logic: prompt building per mode and inline `[P1]` citation parsing, with no I/O |
| `arcanum-verify` | Verify API logic: sentence segmentation, citation attribution, judge prompt and output validation, verdicts, and quote location, with no I/O |
| `arcanum-eval` | Quality metrics, golden datasets, scheduled evaluation |
| `arcanum-chunk-eval` | Chunk inspect API, offline benchmark harness, shadow experiment evaluation |
| `arcanum-engine` | `ArcanumEngine` builder: wires the full system |
| `arcanum-mcp` | MCP JSON-RPC 2.0 server |
| `arcanum-server` | Axum HTTP server, admin portal, WebSocket handler |
| `arcanum-telemetry` | Structured tracing, Prometheus metrics, Grafana stack |

---

## License

MIT; see [LICENSE](LICENSE).
