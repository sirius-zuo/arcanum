# Arcanum — Internal Architecture Wiki

Arcanum is a production-grade Retrieval-Augmented Generation engine written in Rust. It combines five retrieval strategies — dense vector, BM25 lexical, knowledge graph, hierarchical RAPTOR tree, and token-level ColBERT — behind a single orchestrator, with document-level RRF fusion across backends. A hexagonal architecture is enforced at the type level: every storage backend, model provider, and external service sits behind a trait defined in `arcanum-core`, so backends (LanceDB vs. PgVector, Neo4j vs. in-memory Sled) swap with a builder change rather than a pipeline rewrite.

The workspace is sixteen crates layered as a strict DAG. `arcanum-core` holds the shared domain types and ports; `arcanum-vector`, `arcanum-graph`, and `arcanum-tree` implement the storage backends, each with its own chunking strategy; `arcanum-ingestion` and `arcanum-pipeline` turn raw documents into indexed chunks through a DAG stage runner; `arcanum-retrieval` fuses backend results; `arcanum-evidence` traces every chunk back to an exact document version and byte range; and `arcanum-engine` composes it all into services consumed by the REST server and the native MCP server. Shadow chunk-strategy experiments and an offline benchmark harness (`arcanum-chunk-eval`, `arcanum-eval`) let changes be measured before they are committed.

> **Audience:** developers **of** Arcanum itself. Consumer-facing
> documentation (READMEs, tutorials, API docs) lives elsewhere and is not
> duplicated here.

## System Map

```mermaid
graph TD
    server[arcanum-server] --> engine[arcanum-engine]
    server --> chunk-eval[arcanum-chunk-eval]
    server --> telemetry[arcanum-telemetry]
    server --> core[arcanum-core]
    mcp[arcanum-mcp] --> engine
    mcp --> core
    engine --> retrieval[arcanum-retrieval]
    engine --> pipeline[arcanum-pipeline]
    engine --> evidence[arcanum-evidence]
    engine --> eval[arcanum-eval]
    engine --> ingestion[arcanum-ingestion]
    engine --> models[arcanum-models]
    engine --> middleware[arcanum-middleware]
    engine --> vector[arcanum-vector]
    engine --> graph[arcanum-graph]
    engine --> tree[arcanum-tree]
    engine --> core
    pipeline --> ingestion
    pipeline --> tree
    pipeline --> vector
    pipeline --> models
    pipeline --> middleware
    pipeline --> core
    retrieval --> graph
    retrieval --> vector
    retrieval --> core
    evidence --> tree
    evidence --> graph
    evidence --> core
    chunk-eval --> ingestion
    chunk-eval --> core
    ingestion --> core
    middleware --> core
    models --> core
    eval --> core
    vector --> core
    graph --> core
    tree --> core
```

## Page Index

| Page | Covers | Summary |
|------|--------|---------|
| [core](core.md) | `arcanum-core`, `arcanum-models` | Shared domain types, error taxonomy, and the port traits every backend implements; model-provider clients for embedding and generation. |
| [storage](storage.md) | `arcanum-vector`, `arcanum-graph`, `arcanum-tree` | The three storage backends — vector index, knowledge graph, RAPTOR tree — and their pluggable store implementations. |
| [ingestion](ingestion.md) | `arcanum-ingestion` | Document loading and preprocessing: the Docling integration and the name-keyed PreprocessorCatalog. |
| [pipeline](pipeline.md) | `arcanum-pipeline`, `arcanum-middleware` | The DAG stage runner, pipeline templates, per-backend chunk stages, and cross-cutting middleware. |
| [retrieval](retrieval.md) | `arcanum-retrieval` | Retrieval orchestration strategies and document-level RRF fusion across backends. |
| [evidence](evidence.md) | `arcanum-evidence` | Document versioning, raw snapshots, typed chunk provenance, proof chains, and retention GC. |
| [engine](engine.md) | `arcanum-engine` | The ArcanumEngine facade: service composition, auth, audit, events, and circuit breakers. |
| [interfaces](interfaces.md) | `arcanum-server`, `arcanum-mcp`, `arcanum-telemetry` | The REST/WebSocket server, the native MCP server, and observability wiring. |
| [evaluation](evaluation.md) | `arcanum-eval`, `arcanum-chunk-eval` | Retrieval evaluation, the offline chunking benchmark harness, inspect API, and shadow experiments. |

## Maintenance Convention

Every page ends with a **Source Anchors** section listing the paths it
documents. **Rule:** a PR that changes files under a page's anchors either
updates the page or says why not in the PR body. Drift is detectable
mechanically — `git log <last-commit-touching-page>.. -- <anchors>` lists
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
