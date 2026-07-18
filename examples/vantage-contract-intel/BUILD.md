# Production Deployment Guide: Vantage Contract Intelligence

The Full pipeline needs a vector store, graph store, tree store, and model
providers for embeddings + enrichment.

---

## Services to start

| Service | Purpose | Docker image |
|---|---|---|
| PostgreSQL 16 + pgvector | Vector store + RAPTOR tree + metadata | `pgvector/pgvector:pg16` |
| Neo4j 5 | Party/obligation graph | `neo4j:5` |
| HuggingFace TEI | Embeddings | `ghcr.io/huggingface/text-embeddings-inference:cpu-1.5` |
| GLiNER service | Entity extraction (parties, dates, legal terms) | self-hosted |

Claude (Anthropic API) handles ContextPrefix and Summarize.

---

## config.toml changes

```toml
[global]
runtime_mode = "production"

[storage]
metadata_backend = "postgres"
graph_enabled    = true
tree_enabled     = true
```

Leave `orchestration_mode = "ParallelFusion"` and `strategy_timeout_ms = 500`.

---

## Code changes in src/main.rs

```rust
// REMOVE dev stores/enricher:
let vector_store = Arc::new(LanceDbStore::new("data/vantage.lance").await?);
let embedder = Arc::new(OllamaProvider::new(&ollama, "nomic-embed-text", "nomic-embed-text"));
let enricher = Arc::new(OllamaProvider::new(&ollama, "nomic-embed-text", "qwen2.5"));
let graph_store = Arc::new(InMemoryGraphStore::new());
let tree_store = Arc::new(InMemoryTreeStore::new());

// ADD:
let db_url    = std::env::var("DATABASE_URL").expect("DATABASE_URL");
let tei_url   = std::env::var("TEI_URL").expect("TEI_URL");
let neo4j_url = std::env::var("NEO4J_URL").expect("NEO4J_URL");
let neo4j_user = std::env::var("NEO4J_USER").expect("NEO4J_USER");
let neo4j_pw  = std::env::var("NEO4J_PASSWORD").expect("NEO4J_PASSWORD");
let gliner_url = std::env::var("GLINER_URL").expect("GLINER_URL");
let claude_key = std::env::var("ANTHROPIC_API_KEY").expect("ANTHROPIC_API_KEY");

let vector_store = Arc::new(PgVectorStore::new(&db_url, 768).await?);
let embedder = Arc::new(HuggingFaceTeiProvider::new(&tei_url, "nomic-embed-text", 768));
let enricher = Arc::new(
    EnrichmentDispatcher::new(Arc::new(AnthropicProvider::new(&claude_key, "claude-haiku-4-5-20251001")))
        .with_override(EnrichIntent::ExtractEntities, Arc::new(GlinerProvider::new(&gliner_url)))
);
let graph_store = Arc::new(Neo4jStore::new(&neo4j_url, &neo4j_user, &neo4j_pw).await?);
let tree_store = Arc::new(PgTreeStore::new(&db_url).await?);
```

> **Note on the Parties page:** it consumes the framework endpoint `GET /api/v1/graph`,
> typed against `Arc<dyn GraphStore>`. Swapping `InMemoryGraphStore` for `Neo4jStore`
> requires **no** changes; the endpoint serves whichever graph store the engine holds.

Add imports:
```rust
use arcanum_vector::PgVectorStore;
use arcanum_models::{HuggingFaceTeiProvider, EnrichmentDispatcher, AnthropicProvider, GlinerProvider};
use arcanum_core::types::EnrichIntent;
use arcanum_graph::Neo4jStore;
use arcanum_tree::PgTreeStore;
```

---

## Environment variables

| Variable | Required | Description |
|---|---|---|
| `ARCANUM_AUTH_SECRET` | Yes | 32+ char secret |
| `DATABASE_URL` | Yes | postgres connection string |
| `TEI_URL` | Yes | embedding service |
| `NEO4J_URL`, `NEO4J_USER`, `NEO4J_PASSWORD` | Yes | party graph |
| `GLINER_URL` | Yes | entity extraction service |
| `ANTHROPIC_API_KEY` | Yes | ContextPrefix + Summarize |
| `PORT` | No | Default: 8080 |
