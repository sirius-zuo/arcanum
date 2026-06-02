# Folio Library Search

Digital library search and discovery powered by [Arcanum](../../README.md).

**Scenario:** Folio is a 60-person digital library platform for schools and public libraries. Patrons upload books and query across the collection — from exact passages to whole-book summaries to thematic discovery.

**Arcanum configuration:**
- Ingestion: **Full** — author/character/series knowledge graph + RAPTOR tree (passage → chapter summary → book summary) + contextual enrichment
- Retrieval: **ParallelFusion** (RRF, 600 ms timeout) — passage, chapter-summary, book-summary, and graph results coexist

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

---

## Run in development

```bash
npm install --prefix ui
make dev
```

Open **http://localhost:5173**.

---

## Try it

1. **My Library** → upload all excerpts from `samples/`. Watch authors and series appear as entities are extracted.
2. **Search** → try queries across the spectrum, and use the result-type filter:

| Query | Result types you'll see |
|---|---|
| `Call me Ishmael` | **Passage** (exact line) |
| `Tolkien` | **Graph** (author entity) |
| `summarise Moby Dick` | **Book Summary** (RAPTOR L2) |
| `the riddle contest in the dark` | **Passage** / **Chapter Summary** |

3. **Discover** → thematic queries like `books about obsession and fate` — results are grouped, anchored by whole-book summaries.

---

## Build for production

```bash
make build
./target/release/folio-library-search
```

See [BUILD.md](BUILD.md) to switch to production stores (PostgreSQL, Neo4j, TEI, spaCy, Claude).
