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

Press **Tour** in the top bar. Nine steps, each with a deep link, an auto-detected completion check and a result callout. The copy lives in `samples/tour.json`. Progress is kept in your browser; steps that depend on the library (load, update) are taken back when Atlas restarts with empty data, and **Reset tour** on the card clears everything.

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
| `ATLAS_HOST` | `127.0.0.1` | Interface the HTTP server binds. Loopback by default; set `0.0.0.0` only on a trusted network (see [Security](#security-read-this-before-widening-the-bind)) |
| `PORT` | `8080` | HTTP port (API, `/demo` routes, built UI) |
| `MCP_PORT` | `8081` | MCP server port (always bound on all interfaces, see Security) |
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

## Security (read this before widening the bind)

Atlas is a local demo, not a hardened service.

- **Bind address.** The HTTP server (API, `/demo`, UI) binds `127.0.0.1` by default. `ATLAS_HOST=0.0.0.0` opts in to listening on every interface.
- **The demo key is public by design.** `GET /demo/bootstrap` is unauthenticated and returns an admin API key so the UI can sign in. While the server is bound to loopback it refuses requests whose `Host` header is not `localhost`, `*.localhost`, `127.0.0.1` or `[::1]` (403), which blocks DNS-rebinding pages from reading the key. With `ATLAS_HOST` widened that check is off, so anyone who can reach the port gets an admin key.
- **The default signing secret is public.** `ARCANUM_AUTH_SECRET` defaults to `arcanum-dev-secret-minimum-32chars!!`, which is in this repository. Anyone can mint valid keys for an Atlas using it. Use it for local runs only, and set your own secret if the port is reachable by others.
- **MCP port.** The MCP server comes from `arcanum-mcp`, which hard-codes `0.0.0.0:<MCP_PORT>`; Atlas cannot narrow it. On a normal desktop that exposes `/mcp` (tool listing and tool calls, authenticated like the API) to your network even though the HTTP server is loopback only. Block port 8081 with a firewall if that matters.
- **Startup wipes `data/`.** Atlas refuses to start unless `config.toml` and `samples/` exist in the working directory, so it never wipes a `data/` directory elsewhere. A missing or malformed `config.toml` is a startup error.

---

## Honest limits

- **Data resets on every start.** `data/` is wiped at startup unless `ATLAS_KEEP_DATA` is set. The graph, tree and chunk registry are in memory and are not persisted either, so keeping `data/` alone does not restore them.
- **Local model quality.** A small local model gives weaker answers, enrichment and judging than a hosted one. For better results use a larger Ollama model or set `ANTHROPIC_API_KEY`. The prepared flawed answers do not depend on the answer model, only on the judge.
- **Reasoning models (observed with Ollama 0.34.2 and `qwen3.6:35b-a3b-nvfp4`).** Their thinking is streamed separately and used up the whole token budget, so answers were empty and the judge returned nothing. Atlas works around it in two places: the chat and judge client sends `reasoning_effort: "none"` (Ollama 0.34.2 honored it on `/v1/chat/completions`; `think: false` was ignored there) and retries once without it if Ollama answers 400, and enrichment goes through a loopback shim (`src/ollama_shim.rs`) that adds `think: false` to `/api/generate`. Other Ollama versions were not tested; models without a thinking mode ignore both fields. Without the shim the `full` pipeline took 5 to 6 minutes per enrichment call on that model and concurrent calls got HTTP 500. The shim also replaces the framework's entity extraction prompt, which names no JSON keys: the model answered with its own keys inside a code fence, the framework silently parsed that as empty and the graph stayed empty. With the shim, the full pipeline on `qwen3.6:35b-a3b-nvfp4` ingested the ten documents in about 12 minutes and produced a graph of about 100 entities and 34 relations (small models extract few relations).
- **Slow local models.** The `full` pipeline calls the enricher many times, and local generation and judging are slow. `config.toml` raises the generate and verify timeouts for this reason. The Library shows live progress.
- **Metrics are empty (known upstream limitation).** The framework's `/metrics` returns no data in this build because two incompatible `prometheus` crate versions are in the dependency tree (the recorder writes to one registry, the endpoint reads another). The Admin page therefore shows a "no metrics" state and stops polling; it has a Check again button. This is not an Atlas setting and is tracked as a separate task.
- **The engine rate limits a key to 120 requests a minute.** Atlas polls slowly (every 8 s per pending operation, plus socket nudges) and the smoke script paces itself; hammering the API from other scripts with the demo key can return 429.
- **GC is disabled.** `/admin/gc` returns 503 because retention GC needs Postgres-backed stores. The Admin page shows this as a designed state.
- **Search returns one chunk per document** under fusion, and responses carry full vectors, which the UI drops client-side.
- **Orchestration mode is engine-wide config.** It is set at startup in `src/engine_setup.rs` (overriding `config.toml`), not per request.
- **Shadow experiments** need 50 samples before an experiment is `ready_to_promote`; the Lab states this plainly.
- **The operation list is local.** The UI remembers the ingestion operation ids it created in `localStorage`; there is no server-side list.

For moving to production stores, see [BUILD.md](BUILD.md).
