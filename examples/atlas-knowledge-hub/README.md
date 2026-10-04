# Atlas Knowledge Hub

The all-capabilities showcase for [Arcanum](../../README.md).

**Scenario:** Atlas is the internal knowledge hub of Halcyon Robotics, a fictional warehouse-robotics company. You load ten short documents (handbook, security policy, org chart, incident postmortem, product datasheets, runbook, roadmap, vendor contracts, FAQ), then search, build context, ask questions with citations, verify answers sentence by sentence, update a document, trace claims to source bytes, and measure retrieval quality.

**Arcanum configuration:**
- Ingestion: **Full** (`template: "full"`): vector index, BM25 index, knowledge graph and RAPTOR summary tree, with enrichment by a local Ollama model
- Retrieval: **ParallelFusion** (RRF), fixed in code at startup (see [BUILD.md](BUILD.md) to change it)
- Generation and verification: a local Ollama chat model by default, plus Claude when `ANTHROPIC_API_KEY` is set

Dev mode uses LanceDB, in-memory graph, tree and chunk-metadata stores, and a SQLite version store.

---

## What Atlas shows

| Capability | Where in Atlas | Arcanum surface used |
|---|---|---|
| Durable, idempotent ingestion, full pipeline | Library | `/api/v1/ingestion-operations`, polling, `/ws/events` |
| Document versioning and supersession | Library, tour step 7 | `/demo/library` over `DocumentVersionStore`; evidence `version_num` |
| Per-backend chunking, preprocessing | Library, Lab | ingestion pipeline, `/api/v1/chunk/inspect` |
| Vector, BM25, graph, RAPTOR retrieval | Search, Context | `/api/v1/search`, `/api/v1/context` |
| Orchestration modes and RRF fusion | Search, Overview | engine config shown via `/demo/bootstrap` |
| Context packing, numbering, rendering | Context | `/api/v1/context` |
| Generation, streaming, citations | Ask | `/api/v1/generate` (SSE) |
| Verification, strict citations, judge choice | Ask, Verify lab | `/api/v1/verify`, `verify: true` |
| Evidence and provenance | Evidence, Ask, Search | `/evidence/*`, `/demo/documents/.../text` |
| Knowledge graph | Graph | `/api/v1/graph`, `/evidence/entity` |
| Chunk strategy evaluation and shadow experiments | Lab | `/api/v1/chunk/*`, experiments routes |
| Retrieval quality evaluation | Lab | `/demo/eval` over `EvalRunner` |
| Auth, RBAC, audit | Admin, bootstrap | admin key, `/admin/audit` |
| Observability and circuit breakers | Admin | `/demo/metrics`, `/ready`, `/health` |
| Real-time events | Library, Admin | `/ws/events` |
| Retention GC | Admin | `/admin/gc` (disabled state explained) |
| MCP | Connect | MCP server on `MCP_PORT`, `/demo/mcp` |
| Source deletion | Library | `DELETE /api/v1/collections/:c/sources` |

---

## Prerequisites

- Rust stable toolchain
- Node.js 18+
- [Ollama](https://ollama.ai):
  ```bash
  ollama pull nomic-embed-text
  ollama pull qwen2.5
  ```
  `qwen2.5` is the default chat and enrichment model. To use another model, pull it and set `ATLAS_CHAT_MODEL` and `ATLAS_ENRICH_MODEL` (see [Environment variables](#environment-variables)). The embedding model is fixed at `nomic-embed-text` (768 dimensions).
- Optional: `ANTHROPIC_API_KEY`. When set, Atlas adds a `claude` generator and makes it both the default generator and the judge.

No docling-serve is needed: the sample corpus is Markdown, so preprocessing is the identity.

---

## Run in development

```bash
make dev
```

`make dev` starts the backend on `:8080` in the background and the Vite dev server on `:5173` in the foreground; Ctrl-C stops both. Run `npm install --prefix ui` first if `ui/node_modules` does not exist. Open **http://localhost:5173**.

## Run as a single binary

```bash
make build   # npm ci + vite build + cargo build --release
make run     # ./target/release/atlas-knowledge-hub
```

The binary serves the built UI from `ui/dist/` on `http://localhost:8080/`. Run it from this directory, because it reads `config.toml` and `samples/` relative to the working directory.

Other targets: `make test` (cargo tests and UI tests), `make smoke` (end-to-end script against a running Atlas; set `ATLAS_URL` if it is not on `http://localhost:8080`), `make clean`.

---

## Guided tour

Press **Tour** in the top bar. Nine steps, each with a deep link, an auto-detected completion check and a result callout. The copy lives in `samples/tour.json`.

| # | Step | Page | What you see |
|---|---|---|---|
| 1 | Load the Halcyon corpus | Library | Ten documents appear, each with a version and a chunk count |
| 2 | Search by meaning and by exact term | Search | `HX2-BAT-48V charging time` finds the HX-2 datasheet by exact term; the password question finds the security policy without sharing its words |
| 3 | Build grounded context | Context | Numbered passages from the org chart and the runbook, packed into a token budget |
| 4 | Ask a multi-hop question | Ask | An answer to "Who is the on-call lead for the team that owns the navigation stack?" with citations to passages from both documents |
| 5 | Verify the answer | Ask | Each sentence gets a verdict and the supporting quote |
| 6 | Catch a flawed answer | Verify lab | The 80 kg payload sentence is `unsupported`, the HX-1 sentence is `miscited`, the correct sentence passes |
| 7 | Update a document and see versions | Library | The security policy becomes version 2 (180-day rotation, mandatory MFA); the answer changes |
| 8 | Trace a claim to its source | Evidence | The source text opens with the supporting quote highlighted and its version status |
| 9 | Measure retrieval quality | Lab | Hit rate, MRR and NDCG over twelve golden queries |

---

## Sample corpus

All files are in `samples/`. The company, people and products are fictional.

| File | What it demonstrates |
|---|---|
| `employee-handbook.md` | Vector and BM25 retrieval, chunk inspection |
| `security-policy.md` | Versioning: version 1 (rotation every 90 days, MFA optional) |
| `updates/security-policy.md` | Versioning: version 2 (rotation every 180 days, MFA mandatory), ingested under the same source by "Apply policy update" |
| `org-and-teams.md` | Graph retrieval and the graph explorer (multi-hop questions) |
| `incident-2025-03-warehouse-outage.md` | Graph, RAPTOR summaries, evidence |
| `product-hx2-datasheet.md` | BM25 for exact terms, Verify on numeric claims |
| `product-hx1-datasheet.md` | Retrieval ranking, Verify on cross-document miscitations |
| `customer-faq.md` | Context packing, summarize mode |
| `fleet-ops-runbook.md` | Generation with citations |
| `roadmap-2026.md` | RAPTOR (abstractive questions) |
| `vendor-contracts-summary.md` | Retrieval, Verify |

Supporting data: `golden.json` (twelve queries with the source each should hit, used by the Lab evaluation), `flawed-answers.json` (prepared answers for the Verify lab), `tour.json` (the nine steps). Set `ATLAS_SAMPLES_DIR` to read the samples from another directory.

---

## Environment variables

Read by `Settings::from_env` in `src/settings.rs` unless noted.

| Variable | Default | Description |
|---|---|---|
| `PORT` | `8080` | HTTP port (API, `/demo` routes, built UI) |
| `MCP_PORT` | `8081` | MCP server port |
| `OLLAMA_URL` | `http://localhost:11434` | Ollama base URL |
| `ATLAS_CHAT_MODEL` | `qwen2.5` | Local generator and judge model |
| `ATLAS_ENRICH_MODEL` | `qwen2.5` | Model used for enrichment during ingestion |
| `ANTHROPIC_API_KEY` | unset | Adds the `claude` generator and judge |
| `ARCANUM_AUTH_SECRET` | `arcanum-dev-secret-minimum-32chars!!` | Secret used to sign API keys (32+ characters) |
| `ATLAS_KEEP_DATA` | unset | Any value other than empty, `0` or `false` keeps `data/` across restarts |
| `ATLAS_PIPELINE` | `full` | Ingestion pipeline template for the sample loader (read in `src/demo/ingest.rs`) |
| `ATLAS_SAMPLES_DIR` | `samples` | Sample corpus directory (read in `src/samples.rs`) |

The admin API key is minted at startup, printed in the banner and written to `.arcanum-dev-key`.

---

## How the UI maps to the API

The UI calls the real Arcanum API for everything it can. The `/demo` routes exist only for things the framework API does not expose to a browser: the sample corpus, the dev key, a Library view that joins versions with chunk counts, and aggregates over server-side handles.

| Page | Arcanum API | `/demo` helpers (and why) |
|---|---|---|
| Overview (`/`) | none | `/demo/bootstrap` (dev key, mode, generators; the UI has no other way to get a key), `/demo/health` (Ollama and model probe), `/demo/samples/load` |
| Library (`/library`) | `/api/v1/ingestion-operations`, `/ws/events`, `DELETE /api/v1/collections/:c/sources` | `/demo/library` (versions plus chunk counts, no single API call), `/demo/samples/apply-update`, `/demo/samples` |
| Search (`/search`) | `POST /api/v1/search` | none |
| Context (`/context`) | `POST /api/v1/context` | none |
| Ask (`/ask`) | `POST /api/v1/generate` (SSE) | none |
| Verify lab (`/verify`) | `POST /api/v1/verify` | `/demo/samples` (prepared flawed answers) |
| Evidence (`/evidence`) | `/evidence/*` | `/demo/documents/:document_id/versions/:n/text` (canonical text that byte offsets index into) |
| Graph (`/graph`) | `GET /api/v1/graph`, `/evidence/entity/:id` | none |
| Lab (`/lab`) | `/api/v1/chunk/*`, experiments routes | `/demo/eval` (runs the golden set through `EvalRunner`) |
| Admin (`/admin`) | `/admin/audit`, `/admin/rotate-keys`, `/admin/gc`, `/ready`, `/health`, `/ws/events` | `/demo/metrics` (parsed Prometheus snapshot; the metrics token stays server-side) |
| Connect (`/connect`) | MCP on `MCP_PORT` | `/demo/mcp` (endpoint and tool list from `tools/list`) |

`/demo/bootstrap` is unauthenticated and returns the admin key. That is a demo shortcut and must not be copied into a real deployment. The other `/demo` routes require the same bearer key as the API.

---

## Honest limits

- **Data resets on every start.** `data/` is wiped at startup unless `ATLAS_KEEP_DATA` is set. The graph, tree and chunk registry are in memory and are not persisted either, so keeping `data/` alone does not restore them.
- **Local model quality.** A small local model gives weaker answers, enrichment and judging than a hosted one. For better results use a larger Ollama model or set `ANTHROPIC_API_KEY`. The prepared flawed answers do not depend on the answer model, only on the judge.
- **Slow local models.** The `full` pipeline calls the enricher many times, and local generation and judging are slow. `config.toml` raises the generate and verify timeouts for this reason. The Library shows live progress.
- **GC is disabled.** `/admin/gc` returns 503 because retention GC needs Postgres-backed stores. The Admin page shows this as a designed state.
- **Search returns one chunk per document** under fusion, and responses carry full vectors, which the UI drops client-side.
- **Orchestration mode is engine-wide config.** It is set at startup in `src/engine_setup.rs` (overriding `config.toml`), not per request.
- **Shadow experiments** need 50 samples before an experiment is `ready_to_promote`; the Lab states this plainly.
- **The operation list is local.** The UI remembers the ingestion operation ids it created in `localStorage`; there is no server-side list.

For moving to production stores, see [BUILD.md](BUILD.md).
