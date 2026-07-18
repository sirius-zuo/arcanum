# Production Deployment Guide: Folio Library Search

The Full pipeline needs a vector store, graph store, tree store, and model
providers for embeddings + enrichment.

---

## Services to start

| Service | Purpose | Docker image |
|---|---|---|
| PostgreSQL 16 + pgvector | Vector store + RAPTOR tree + metadata | `pgvector/pgvector:pg16` |
| Neo4j 5 | Author/character/series graph | `neo4j:5` |
| HuggingFace TEI | Embeddings | `ghcr.io/huggingface/text-embeddings-inference:cpu-1.5` |
| spaCy service | NER (Author, Character, Place, Series) | self-hosted |

Claude (Anthropic API) handles ContextPrefix (book + chapter context) and
Summarize (RAPTOR summaries).

---

## config.toml changes

```toml
[global]
runtime_mode = "production"

[storage]
metadata_backend = "postgres"
graph_enabled    = true
tree_enabled     = true

[tree]
raptor_max_depth    = 3
raptor_cluster_size = 15   # larger clusters for full-length novels
```

Leave `orchestration_mode = "ParallelFusion"` and `strategy_timeout_ms = 600`.

---

## Code changes in src/main.rs

```rust
// REMOVE dev stores/enricher:
let vector_store = Arc::new(LanceDbStore::new("data/folio.lance").await?);
let embedder = Arc::new(OllamaProvider::new(&ollama, "nomic-embed-text", "nomic-embed-text"));
let enricher = Arc::new(OllamaProvider::new(&ollama, "nomic-embed-text", "qwen2.5"));
let graph_store = Arc::new(InMemoryGraphStore::new());
let tree_store = Arc::new(InMemoryTreeStore::new());
let version_store = Arc::new(SqliteDocumentVersionStore::open("data/versions.db").await?);
let chunk_metadata_store = Arc::new(InMemoryChunkMetadataStore::new());

// ADD:
let db_url    = std::env::var("DATABASE_URL").expect("DATABASE_URL");
let tei_url   = std::env::var("TEI_URL").expect("TEI_URL");
let neo4j_url = std::env::var("NEO4J_URL").expect("NEO4J_URL");
let neo4j_user = std::env::var("NEO4J_USER").expect("NEO4J_USER");
let neo4j_pw  = std::env::var("NEO4J_PASSWORD").expect("NEO4J_PASSWORD");
let spacy_url = std::env::var("SPACY_URL").expect("SPACY_URL");
let claude_key = std::env::var("ANTHROPIC_API_KEY").expect("ANTHROPIC_API_KEY");

let vector_store = Arc::new(PgVectorStore::new(&db_url, 768).await?);
let embedder = Arc::new(HuggingFaceTeiProvider::new(&tei_url, "nomic-embed-text", 768));
let enricher = Arc::new(
    EnrichmentDispatcher::new(Arc::new(AnthropicProvider::new(&claude_key, "claude-haiku-4-5-20251001")))
        .with_override(EnrichIntent::ExtractEntities, Arc::new(SpacyProvider::new(&spacy_url)))
);
let graph_store = Arc::new(Neo4jStore::new(&neo4j_url, &neo4j_user, &neo4j_pw).await?);
let tree_store = Arc::new(PgTreeStore::new(&db_url).await?);
let version_store = Arc::new(PostgresDocumentVersionStore::new(&db_url).await?);
let snapshot_store = Arc::new(LocalSnapshotStore::new("data/snapshots")); // or an S3-backed SnapshotStore
let chunk_metadata_store = Arc::new(PostgresChunkMetadataStore::new(&db_url).await?);

// The evidence resolver and GC worker are built from the same stores wired above;
// no separate config. GC enforces RetentionBased versioning policy and needs Postgres
// for its bookkeeping, so (unlike the resolver) it isn't available in the dev example.
let gc_worker = Arc::new(PostgresGcWorker::new(
    &db_url, version_store.clone(), snapshot_store, vector_store.clone(),
    tree_store.clone(), graph_store.clone(), chunk_metadata_store.clone(),
).await?);
// .gc_worker(gc_worker): add to the builder chain alongside .evidence(...)
```

> **Note on full-length books:** RAPTOR build time for a 300,000-word novel is
> 5–15 minutes per book depending on the Summarize enricher speed. For large
> corpora, run RAPTOR builds as async background jobs separate from the main
> ingestion path.
>
> **Note on author/series browsing:** it consumes the framework endpoint
> `GET /api/v1/graph`, typed against `Arc<dyn GraphStore>`. Swapping
> `InMemoryGraphStore` for `Neo4jStore` requires **no** changes; the endpoint serves
> whichever graph store the engine holds.

Add imports:
```rust
use arcanum_vector::PgVectorStore;
use arcanum_models::{HuggingFaceTeiProvider, EnrichmentDispatcher, AnthropicProvider, SpacyProvider};
use arcanum_core::types::EnrichIntent;
use arcanum_graph::Neo4jStore;
use arcanum_tree::PgTreeStore;
use arcanum_ingestion::{PostgresDocumentVersionStore, PostgresChunkMetadataStore, PostgresGcWorker, LocalSnapshotStore};
```

---

## Environment variables

| Variable | Required | Description |
|---|---|---|
| `ARCANUM_AUTH_SECRET` | Yes | 32+ char secret |
| `DATABASE_URL` | Yes | postgres connection string |
| `TEI_URL` | Yes | embedding service |
| `NEO4J_URL`, `NEO4J_USER`, `NEO4J_PASSWORD` | Yes | author/series graph |
| `SPACY_URL` | Yes | NER service |
| `ANTHROPIC_API_KEY` | Yes | ContextPrefix + Summarize |
| `PORT` | No | Default: 8080 |
