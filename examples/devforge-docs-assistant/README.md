# Devforge Docs Assistant

Developer portal search powered by [Arcanum](../../README.md).

**Scenario:** Devforge is a 25-person API-first SaaS startup. This assistant lets developers search API documentation, error references, and SDK guides using natural language or exact terms.

**Arcanum configuration:**
- Ingestion: **Standard** (Load → Chunk → Embed → VectorWrite)
- Retrieval: **Static** ([Vector, BM25]) — fixed two-strategy set, no classifier overhead

---

## Prerequisites

- Rust stable toolchain
- Node.js 20+
- [Ollama](https://ollama.ai) running locally with `nomic-embed-text` pulled:
  ```bash
  ollama pull nomic-embed-text
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
# Install UI dependencies (first time only)
npm install --prefix ui

# Start backend (port 8080) and UI dev server (port 5173)
make dev
```

Then open **http://localhost:5173** in your browser.

The terminal will print your dev API key — it's automatically injected into the UI.

---

## Try it

1. Go to **Docs** and drop any file from `samples/` onto the upload zone
2. Wait for status to show **Ready**
3. Go to **Search** and try these queries:

| Query | Expected strategy |
|---|---|
| `invalid_api_key error` | **BM25** — exact phrase match |
| `how do I authenticate with OAuth2?` | **Vector** — semantic match |
| `rate limit headers` | **Both** — overlap |

The strategy badge on each result shows which retriever found it.

---

## Build for production

```bash
make build
./target/release/devforge-docs-assistant
```

See [BUILD.md](BUILD.md) to switch to production stores (PostgreSQL, TEI).
