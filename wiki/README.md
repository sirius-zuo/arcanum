# Arcanum — Internal Architecture Wiki

Arcanum is a production-grade grounded RAG engine written in Rust: retrieval, [context](context.md), [generation](generate.md) and [verification](verify.md), each traced to source document versions and offsets. `arcanum-retrieval` defines five retrieval strategies (dense vector, BM25 lexical, knowledge graph, hierarchical RAPTOR tree, and token-level ColBERT) behind a single orchestrator with document-level RRF fusion, though (see [Retrieval](retrieval.md)'s Implementation Notes) `ArcanumEngineBuilder` wires at most four of the five in practice, and two strategies don't yet participate correctly in the document-level fusion they're meant to share. A hexagonal architecture is enforced at the type level: every storage backend, model provider, and external service sits behind a trait defined in `arcanum-core`, so backends (LanceDB vs. PgVector, Neo4j vs. in-memory Sled) swap with a builder change rather than a pipeline rewrite.

The workspace is nineteen crates layered as a strict DAG. `arcanum-core` holds the shared domain types and ports; `arcanum-vector`, `arcanum-graph`, and `arcanum-tree` implement the storage backends, each with its own chunking strategy; `arcanum-ingestion` and `arcanum-pipeline` turn raw documents into indexed chunks through a DAG stage runner; `arcanum-retrieval` fuses backend results; `arcanum-evidence` traces every chunk back to an exact document version and byte range; `arcanum-verify` checks generated answers sentence by sentence against their passages; and `arcanum-engine` composes it all into services consumed by the REST server and the native MCP server. Shadow chunk-strategy experiments and an offline benchmark harness (`arcanum-chunk-eval`, `arcanum-eval`) let changes be measured before they are committed.

> **Audience:** developers **of** Arcanum itself. Consumer-facing
> documentation (READMEs, tutorials, API docs) lives elsewhere and is not
> duplicated here.

## System Map

```mermaid
graph TD
    server[arcanum-server] --> engine[arcanum-engine]
    server --> chunk-eval[arcanum-chunk-eval]
    server --> core[arcanum-core]
    mcp[arcanum-mcp] --> engine
    mcp --> core
    engine --> retrieval[arcanum-retrieval]
    engine --> context[arcanum-context]
    engine --> generate[arcanum-generate]
    engine --> verify[arcanum-verify]
    engine --> pipeline[arcanum-pipeline]
    engine --> evidence[arcanum-evidence]
    engine --> eval[arcanum-eval]
    engine --> ingestion[arcanum-ingestion]
    engine --> models[arcanum-models]
    engine --> middleware[arcanum-middleware]
    engine --> vector[arcanum-vector]
    engine --> graphComp[arcanum-graph]
    engine --> tree[arcanum-tree]
    engine --> core
    pipeline --> ingestion
    pipeline --> tree
    pipeline --> vector
    pipeline --> models
    pipeline --> middleware
    pipeline --> core
    retrieval --> core
    context --> core
    generate --> core
    verify --> core
    verify --> context
    verify --> generate
    evidence --> core
    chunk-eval --> ingestion
    chunk-eval --> core
    ingestion --> core
    middleware --> core
    models --> core
    eval --> core
    vector --> core
    graphComp --> core
    tree --> core
```

## Page Index

| Page | Covers | Summary |
|------|--------|---------|
| [core](core.md) | `arcanum-core`, `arcanum-models` | Shared domain types, the error taxonomy, layered `ArcanumConfig`, and the port traits (`VectorStore`, `Chunker`, `Embedder`, `EvidenceResolver`, and more) every backend is written against; `arcanum-models`' nine provider implementations of the `Embedder`/`TextEnricher` ports. |
| [storage](storage.md) | `arcanum-vector`, `arcanum-graph`, `arcanum-tree` | The concrete storage backends (`arcanum-vector`'s LanceDB/PgVector stores and BM25 lexical index, `arcanum-graph`'s in-memory/Sled/Neo4j graph stores, `arcanum-tree`'s RAPTOR-tree builder), each implementing `arcanum-core`'s storage port traits. |
| [ingestion](ingestion.md) | `arcanum-ingestion` | Document loading, Docling preprocessing, five name-keyed chunking strategies, and the persistence layer (document version history, raw/canonical snapshots, and per-chunk provenance) that makes re-ingestion idempotent. |
| [pipeline](pipeline.md) | `arcanum-pipeline`, `arcanum-middleware` | The DAG stage runner and executor that turn one `IngestionTask` into a completed ingest, the pipeline-template registry, `IngestionWorker`'s queue and durable operation lifecycle, and the `arcanum-middleware` reliability primitives (`BoundedQueue`, `CircuitBreaker`, and the currently unused `RetryPolicy`) backing it. |
| [retrieval](retrieval.md) | `arcanum-retrieval` | `RetrievalOrchestrator` runs a configurable subset of four independent strategy retrievers (vector, BM25, graph, RAPTOR, plus a ColBERT re-rank variant of vector) in parallel and merges their hits with document-level RRF fusion. |
| [evidence](evidence.md) | `arcanum-evidence` | Resolves a chunk, tree node, entity, or relation back to the raw source bytes it came from via `DefaultEvidenceResolver`, returning an auditable `ProofChain`; also holds `PostgresGcWorker`, which purges superseded document versions and the data they own. |
| [context](context.md) | `arcanum-context` | Turns pre-fusion retrieval candidates into a token-budgeted, citation-mapped context of exact source-document passages, optionally rendered for an LLM prompt; `arcanum-context` is pure (depends only on `arcanum-core`), while `ContextService` in `arcanum-engine` adds authorization, the circuit breaker, registry hydration and audit. |
| [generate](generate.md) | `arcanum-generate` | The built-in grounded answer path: `arcanum-generate` (pure prompt building and citation parsing), `arcanum-models`' two provider adapters and SSE line parser, and `GenerateService`, which gets passages from Context, streams the answer, and maps its inline `[P1]` markers to passages, chunk ids, document versions and byte offsets. |
| [verify](verify.md) | `arcanum-verify` | Checks an answer sentence by sentence against the passages it was generated from and ties each verdict to a chunk, a document version and a byte range; `arcanum-verify` is a pure crate, while `VerifyService` in `arcanum-engine` owns auth, registry reads, judge calls, breakers and audit. |
| [engine](engine.md) | `arcanum-engine` | The composition root: `ArcanumEngineBuilder::build` wires every configured store/provider into a running `ArcanumEngine`: pipeline workers, the retrieval orchestrator, per-domain services, and the cross-cutting auth, audit, events, and circuit breakers they share. It wires `ContextService`, `GenerateService` and `VerifyService`; the crates behind them are covered on [context](context.md), [generate](generate.md) and [verify](verify.md). |
| [interfaces](interfaces.md) | `arcanum-server`, `arcanum-mcp`, `arcanum-telemetry` | The workspace's outward-facing edge: `arcanum-server`'s REST/WebSocket API, `arcanum-mcp`'s native JSON-RPC MCP server, and `arcanum-telemetry`'s tracing/metrics wiring. |
| [evaluation](evaluation.md) | `arcanum-eval`, `arcanum-chunk-eval` | `arcanum-eval`'s scaffolded retrieval-quality metrics and scheduler, `arcanum-chunk-eval`'s deterministic chunking-strategy inspect/benchmark harness, and the shadow-experiment lifecycle built on top of it. |

## Maintenance Convention

Every page ends with a **Source Anchors** section listing the paths it
documents. **Rule:** a PR that changes files under a page's anchors either
updates the page or says why not in the PR body. Drift is detectable
mechanically: `git log <last-commit-touching-page>.. -- <anchors>` lists
pages whose sources moved without them; the `generate-wiki` skill's
`refresh` mode automates this. There is deliberately no CI freshness gate:
gates train contributors to make no-op doc edits. Run the materialized
`check-wiki.sh` (in `scripts/` or alongside this file) to verify
structural conventions.

## Page Conventions

Copy [TEMPLATE.md](TEMPLATE.md) for new pages: eight sections in order;
Mermaid-only diagrams; no line numbers (function/type/file names only);
links target only canonical page filenames; every Key Decision cites a real
PR number or commit SHA; known debt appears only under Implementation
Notes. Target 150–350 lines per page; if a draft exceeds ~400 lines it is
over-scoped.
