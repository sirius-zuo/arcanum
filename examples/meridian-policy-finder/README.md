# Meridian Policy Finder

Internal HR/policy Q&A assistant powered by [Arcanum](../../README.md).

**Scenario:** Meridian Consulting is a 150-person management consulting firm. This assistant lets employees ask HR, IT, and benefits questions and get answers with citations, without digging through PDFs.

**Arcanum configuration:**
- Ingestion: **Standard** (Load → Chunk → Embed → VectorWrite)
- Retrieval: **QueryClassified**: a lightweight classifier routes keyword lookups to BM25 and conceptual questions to Vector

---

## Prerequisites

- Rust stable toolchain
- Node.js 20+
- [Ollama](https://ollama.ai) with `nomic-embed-text`:
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
npm install --prefix ui
make dev
```

Open **http://localhost:5173**.

---

## Try it

1. Go to **Policy Library**, choose a category, and upload the files from `samples/`
2. Go to **Ask a Question** and try:

| Question | Routed to |
|---|---|
| `PTO accrual rate for part-time employees` | **BM25**: keyword lookup |
| `how does maternity leave affect my performance review?` | **Vector**: semantic |
| `401k match percentage` | **BM25**: keyword lookup |
| `can I take unpaid leave during an active project?` | **Vector**: semantic |

The **routing pill** below the search bar shows which strategy the classifier chose. Check **Recent Questions** to see your history with the routing decision per query.

---

## Build for production

```bash
make build
./target/release/meridian-policy-finder
```

See [BUILD.md](BUILD.md) to switch to production stores (PostgreSQL, TEI).
