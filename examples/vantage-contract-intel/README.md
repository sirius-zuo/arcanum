# Vantage Contract Intelligence

Contract portfolio intelligence powered by [Arcanum](../../README.md).

**Scenario:** Vantage Legal is the 120-person legal team at a midsize PE firm. Lawyers query the full contract portfolio: clauses, obligations, parties, and precedents.

**Arcanum configuration:**
- Ingestion: **Full**: entity extraction (parties) + knowledge graph + RAPTOR + contextual enrichment
- Retrieval: **ParallelFusion** (RRF, 500 ms per-strategy timeout): all four strategies run per query

Dev mode uses **in-memory** graph and tree stores plus an Ollama enricher.

---

## Prerequisites

- Rust stable toolchain
- Node.js 20+
- [Ollama](https://ollama.ai):
  ```bash
  ollama pull nomic-embed-text
  ollama pull qwen2.5
  ```
- [docling-serve](https://github.com/docling-project/docling-serve) running locally:
  1. Pull the image (~2.7 GB):
     ```bash
     docker pull quay.io/docling-project/docling-serve
     ```
  2. Run it:
     ```bash
     docker run -p 5001:5001 quay.io/docling-project/docling-serve
     ```
  3. Verify it's up:
     ```bash
     curl http://localhost:5001/health
     ```
  For GPU acceleration, use `quay.io/docling-project/docling-serve-cu128` instead (RapidOCR may need container patching for true CUDA support).

---

## Run in development

```bash
npm install --prefix ui
make dev
```

Open **http://localhost:5173**.

---

## Try it

1. **Contract Library** → upload all files from `samples/`. Watch the party count rise.
2. **Search Clauses** → try queries spanning all four strategies:

| Query | Strongest contributor |
|---|---|
| `indemnification cap` | **BM25**: exact term |
| `data residency obligations` | **Vector**: semantic |
| `obligations that survive termination` | **RAPTOR**: clause-group synthesis |
| `TechCorp` | **Graph**: party entity |

Each result shows a four-strategy contribution sidebar (BM25 · Vector · Graph · RAPTOR).

3. **Parties** → browse extracted parties and their relationships.

---

## Build for production

```bash
make build
./target/release/vantage-contract-intel
```

See [BUILD.md](BUILD.md) to switch to production stores (PostgreSQL, Neo4j, TEI, GLiNER, Claude).
