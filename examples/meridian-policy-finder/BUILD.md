# Production Deployment Guide: Meridian Policy Finder

Replace the dev in-memory/local stores with production-grade services.

---

## Services to start

| Service | Purpose | Docker image |
|---|---|---|
| PostgreSQL 16 + pgvector | Vector store + metadata | `pgvector/pgvector:pg16` |
| HuggingFace TEI | Embeddings | `ghcr.io/huggingface/text-embeddings-inference:cpu-1.5` |

```yaml
# docker-compose.yml
services:
  postgres:
    image: pgvector/pgvector:pg16
    environment:
      POSTGRES_DB: arcanum
      POSTGRES_USER: arcanum
      POSTGRES_PASSWORD: changeme
    ports: ["5432:5432"]
  tei:
    image: ghcr.io/huggingface/text-embeddings-inference:cpu-1.5
    command: --model-id BAAI/bge-base-en-v1.5 --port 8081
    ports: ["8081:8081"]
```

---

## config.toml changes

```toml
[global]
runtime_mode = "production"

[storage]
metadata_backend = "postgres"
```

Leave `orchestration_mode = "QueryClassified"` unchanged.

---

## Code changes in src/main.rs

```rust
// REMOVE:
let vector_store = Arc::new(LanceDbStore::new("data/meridian.lance").await?);
let embedder = Arc::new(OllamaProvider::new(&ollama, "nomic-embed-text", "nomic-embed-text"));

// ADD:
let db_url  = std::env::var("DATABASE_URL").expect("DATABASE_URL required");
let tei_url = std::env::var("TEI_URL").expect("TEI_URL required");
let vector_store = Arc::new(PgVectorStore::new(&db_url, 768).await?);
let embedder = Arc::new(HuggingFaceTeiProvider::new(&tei_url, "nomic-embed-text", 768));
```

Add imports:
```rust
use arcanum_vector::PgVectorStore;
use arcanum_models::HuggingFaceTeiProvider;
```

---

## Environment variables

| Variable | Required | Description |
|---|---|---|
| `ARCANUM_AUTH_SECRET` | Yes | 32+ character secret |
| `DATABASE_URL` | Yes | `postgres://arcanum:password@localhost/arcanum` |
| `TEI_URL` | Yes | `http://localhost:8081` |
| `PORT` | No | Default: 8080 |

---

## Running

```bash
ARCANUM_AUTH_SECRET=your-secret \
DATABASE_URL=postgres://arcanum:changeme@localhost/arcanum \
TEI_URL=http://localhost:8081 \
./target/release/meridian-policy-finder
```
