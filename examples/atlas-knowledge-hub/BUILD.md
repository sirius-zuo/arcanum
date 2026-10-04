# Production Deployment Guide: Atlas Knowledge Hub

Atlas is a demo: it wipes its data at startup, runs on in-memory graph and tree stores, and hands out an admin key from an unauthenticated route. This guide covers the changes that matter when you adapt it. Store swaps follow the other examples, so this file points to them instead of repeating their code.

---

## Switching orchestration mode

Atlas forces `ParallelFusion` in `src/engine_setup.rs`:

```rust
config.retrieval.orchestration_mode = OrchestrationMode::ParallelFusion;
```

This line overrides `config.toml`. To try `Static` or `QueryClassified`, change that line (or remove it and set `orchestration_mode` under `[retrieval]` in `config.toml`), then restart. The mode is engine-wide, so every Search, Context and Ask call uses it. Overview and Search display the active mode from `/demo/bootstrap`.

Full ingestion with `Static` retrieval discards the graph and RAPTOR tree at query time; see the coverage notes in [EXAMPLES.md](../../EXAMPLES.md).

---

## Using Anthropic

```bash
export ANTHROPIC_API_KEY=sk-ant-...
make run
```

With the key set, `ModelDeps::ollama` in `src/engine_setup.rs` adds a `claude` generator (model `claude-sonnet-5-5`) and makes it the default generator and the judge. The local generator stays selectable in the Ask page. Embeddings and enrichment still use Ollama. To move enrichment to a hosted model as well, replace the enricher as shown in the [Folio BUILD.md](../folio-library-search/BUILD.md) (`EnrichmentDispatcher` with `AnthropicProvider`).

The generate and verify timeouts in `config.toml` are raised for slow local models; a hosted model can use the defaults (30 and 120 seconds first-token and total, 90 for the judge), so remove those overrides.

---

## Moving to production stores

| Atlas dev store | Production replacement | Pointer |
|---|---|---|
| `LanceDbStore` (`data/atlas.lance`) | `PgVectorStore` (PostgreSQL 16 + pgvector) | [Folio BUILD.md](../folio-library-search/BUILD.md) |
| `InMemoryGraphStore` | `Neo4jStore` | [Folio BUILD.md](../folio-library-search/BUILD.md) |
| `InMemoryTreeStore` | `PgTreeStore` | [Folio BUILD.md](../folio-library-search/BUILD.md) |
| `SqliteDocumentVersionStore` (`data/versions.db`) | `PostgresDocumentVersionStore` | [Folio BUILD.md](../folio-library-search/BUILD.md) |
| `InMemoryChunkMetadataStore` | `PostgresChunkMetadataStore` | [Folio BUILD.md](../folio-library-search/BUILD.md) |
| `LocalSnapshotStore` (`data/snapshots`) | `LocalSnapshotStore` on a persistent volume, or an S3-backed `SnapshotStore` | [Folio BUILD.md](../folio-library-search/BUILD.md) |

Atlas also builds an in-process BM25 index and a local operation payload store (`data/payloads`); keep them on persistent storage if you keep `data/`.

Secrets belong in a secret manager such as Vault rather than in environment files: set `ARCANUM_AUTH_SECRET` from it, and set `ANTHROPIC_API_KEY` the same way. Set `runtime_mode = "production"` under `[global]` in `config.toml`.

Remove the demo shortcuts before deploying:

- `/demo/bootstrap` returns the admin key without authentication.
- `build_state` wipes `data/` at startup unless `ATLAS_KEEP_DATA` is set.
- `.arcanum-dev-key` is written at startup.
- `ARCANUM_AUTH_SECRET` has a public default.

Keeping `data/` with `ATLAS_KEEP_DATA` is not enough to survive a restart in the dev wiring, because the graph, tree and chunk registry are in memory. The Postgres and Neo4j stores above remove that limit.

---

## Enabling GC

`POST /admin/gc` returns 503 in Atlas because no GC worker is wired. Retention GC needs Postgres-backed stores, so first do the store swaps above. Then build a `PostgresGcWorker` from the same stores and add it to the builder chain in `src/engine_setup.rs`, exactly as in the Folio guide:

```rust
let gc_worker = Arc::new(PostgresGcWorker::new(
    &db_url, version_store.clone(), snapshot_store, vector_store.clone(),
    tree_store.clone(), graph_store.clone(), chunk_metadata_store.clone(),
).await?);
// builder: .gc_worker(gc_worker)
```

GC enforces the `RetentionBased` versioning policy. Chunks collected by GC disappear from Search and from evidence; Verify reports passages whose chunk was collected as unavailable.

---

## MCP

The MCP server listens on `MCP_PORT` (default `8081`) at `/mcp`. The Connect page lists its tools and a copyable client config. In production, put it behind the same TLS and auth boundary as the API.
