# Canopy Product Search

Product + support search powered by [Arcanum](../../README.md).

**Scenario:** Canopy Co. is an 80-person D2C outdoor gear brand. This search assistant serves two audiences at once: customers browsing by description, and support agents looking up exact SKUs.

**Arcanum configuration:**
- Ingestion: **Standard**
- Retrieval: **ParallelFusion** (RRF) — Vector and BM25 both run on every query; results are fused so semantic and keyword matches coexist

---

## Prerequisites

- Rust stable toolchain
- Node.js 20+
- [Ollama](https://ollama.ai) with `nomic-embed-text`:
  ```bash
  ollama pull nomic-embed-text
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

1. Go to **Catalog** and upload all files from `samples/`
2. On **Search** (customer view), try semantic queries:
   - `best jacket for winter hiking`
   - `lightweight tent for solo backpacking`
   - `warmest sleeping bag for extreme cold`
3. On **Support Lookup** (agent view), try exact lookups:
   - `SKU TN-4892 weight`
   - `SKU JK-2201 waterproof rating`
   - `warranty claim process`

Hover any product card on the Search page to see the **fusion breakdown** — how much BM25 vs Vector contributed.

---

## Build for production

```bash
make build
./target/release/canopy-product-search
```

See [BUILD.md](BUILD.md) to switch to production stores (PostgreSQL, TEI, optional Redis cache).
