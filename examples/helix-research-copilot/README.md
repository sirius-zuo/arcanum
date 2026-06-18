# Helix Research Copilot

Research intelligence powered by [Arcanum](../../README.md).

**Scenario:** Helix Labs is a 200-person drug discovery startup. Scientists query the company's research knowledge — compound assays, clinical trials, and mechanisms.

**Arcanum configuration:**
- Ingestion: **Full** — entity extraction + knowledge graph + RAPTOR tree + contextual enrichment
- Retrieval: **QueryClassified** — entity queries route to Graph, synthesis queries to RAPTOR, broad queries to Vector + BM25

Dev mode uses **in-memory** graph and tree stores plus an Ollama enricher, so the full pipeline runs with no external services.

---

## Prerequisites

- Rust stable toolchain
- Node.js 20+
- [Ollama](https://ollama.ai) with an embedding model and a generation model:
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

1. **Research Corpus** → drop all files from `samples/`. Watch the entity count rise as the Full pipeline extracts entities.
2. **Copilot** → try the three query types and watch the routing panel:

| Query | Routes to |
|---|---|
| `does Compound 17g inhibit EGFR?` | **Graph** — entity traversal |
| `summarise adverse events across all Phase 2 trials` | **RAPTOR** — document synthesis |
| `CRISPR delivery mechanisms in neuronal tissue` | **Vector + BM25** — semantic |

3. **Knowledge Graph** → see the extracted entities (compounds, proteins) and their relationships. Click a node to inspect it.

---

## Build for production

```bash
make build
./target/release/helix-research-copilot
```

See [BUILD.md](BUILD.md) to switch to production stores (PostgreSQL, Neo4j, TEI, GLiNER, Claude).
