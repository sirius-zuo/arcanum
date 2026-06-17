# Arcanum Usage Examples

**Date:** 2026-06-01 (updated 2026-06-16)
**Status:** Approved — all 6 examples implemented in `examples/`

Six real-world scenarios for small and midsize companies covering every combination of ingestion strategy (Standard, Full) and retrieval orchestration mode (Static, QueryClassified, ParallelFusion).

---

## How to read these examples

Each example explains:
- **Why this ingestion strategy** — the reasoning for Standard vs. Full, not just which one
- **Why this retrieval mode** — when the mode fits and when it would fail
- **Engine configuration** — minimal working Rust builder + TOML config, matching the actual `examples/<name>/src/main.rs`
- **Representative queries** — showing how each strategy contributes
- **Wrong choice cost** — what breaks if you pick differently

**Coverage map:**

| # | Company | Example dir | Ingestion | Retrieval |
|---|---|---|---|---|
| 1 | Devforge — developer docs assistant | `examples/devforge-docs-assistant` | Standard | Static |
| 2 | Meridian Consulting — internal policy Q&A | `examples/meridian-policy-finder` | Standard | QueryClassified |
| 3 | Canopy Co. — product & support search | `examples/canopy-product-search` | Standard | ParallelFusion |
| 4 | Helix Labs — research intelligence | `examples/helix-research-copilot` | Full | QueryClassified |
| 5 | Vantage Legal — contract intelligence | `examples/vantage-contract-intel` | Full | ParallelFusion |
| 6 | Folio — digital library service | `examples/folio-library-search` | Full | ParallelFusion |

The Full + Static combination is absent because it is an anti-pattern: building a graph and RAPTOR tree only to route queries to a fixed single retriever discards the investment made at ingestion time.

---

## Shared project structure

All 6 examples follow the same structure (each is a standalone Rust binary crate, not a workspace member — devforge declares this with an empty `[workspace]` table in its `Cargo.toml`; the others simply omit one):

```
examples/<example-name>/
├── Cargo.toml              # [package] + [[bin]] — path deps to arcanum-*
├── README.md / BUILD.md    # scenario + run instructions / production migration guide
├── config.toml             # dev configuration
├── Makefile                # make dev, make build, make run, make clean
├── .gitignore               # data/, .arcanum-dev-key, ui/node_modules, ui/dist, target/
├── samples/                # scenario-specific text files for the demo
├── src/main.rs             # ArcanumEngineBuilder + axum server + startup banner
└── ui/
    └── src/
        ├── api/             # client.ts, search.ts, ingest.ts (+ collections, auth)
        ├── components/
        └── pages/
```

**Dependencies** (`Cargo.toml`): Standard examples (Devforge, Meridian, Canopy) depend on `arcanum-engine`, `arcanum-server`, `arcanum-core`, `arcanum-vector`, `arcanum-models`, `arcanum-ingestion`, `arcanum-telemetry`. Full-pipeline examples (Helix, Vantage, Folio) add `arcanum-graph` and `arcanum-tree`; Folio additionally adds `arcanum-evidence` for its evidence resolver.

**`main.rs` pattern:** load `config.toml` (fall back to `ArcanumConfig::default()`) → construct dev stores (with comments pointing to the production alternative — see each example's `BUILD.md`) → `ArcanumEngineBuilder::new(config)` with the stores → mint a real admin API key via `engine.auth.generate_admin_key(...)` and write it to `.arcanum-dev-key` / `ui/.env.development` (a fabricated key string would fail `validate_api_key`) → `arcanum_server::build_app(Some(engine))` → serve `ui/dist/` as a SPA fallback if it exists, otherwise print the Vite dev URL.

**API client pattern** (`ui/src/api/`): all requests carry `Authorization: Bearer <key>`. The core endpoints are `POST /api/v1/search` (body: `query`, `collection_id`, `top_k`) and `POST /api/v1/ingest` (body: `source_uri`, `collection_id`, **`pipeline`** — not `pipeline_template`). Devforge's implementation has grown beyond this minimal shape to support its document-management UI: `POST /api/v1/upload` for raw file bytes, plus `GET/POST/DELETE /api/v1/vector/collections[/:name][/stats|/documents]` for collection and document management. Full-pipeline examples additionally call `GET /api/v1/graph`.

**Dev workflow:** `npm install --prefix ui` → `make dev` (or `cargo run` + `cd ui && npm run dev` in two terminals) → backend on `:8080`, Vite on `:5173` (proxies `/api`, `/admin`, `/ws`, `/evidence` to `:8080`) → open `http://localhost:5173`, ingest from `samples/`, run queries.

**Production workflow:** `make build` (builds `ui/dist/` then `cargo build --release`) → set `ARCANUM_AUTH_SECRET` (32+ chars), `PORT`, and production store URLs (see each `BUILD.md`) → run the release binary, which detects `ui/dist/` and serves API + SPA from one port.

---

## Evidence & versioning layer

Added after this doc's original design (commits `3d552332`, `0b3108cc`, `4a001b88`, `48e42559`; see `docs/superpowers/specs/2026-06-15-evidence-foundation-design.md` and `2026-06-15-evidence-phase2-design.md`). It is engine-level infrastructure, wired in via additional `ArcanumEngineBuilder` methods:

- `.version_store(Arc<dyn DocumentVersionStore>)` — tracks ingestion events, content hashes, and document history per collection (`SqliteDocumentVersionStore` in dev, `PostgresDocumentVersionStore` in production). Powers accurate document counts (`GET /api/v1/vector/collections/:name/documents`, `/stats`) even for documents that produced zero chunks.
- `.snapshot_store(Arc<dyn SnapshotStore>)` — persists the raw bytes + canonical sidecar JSON of each ingested document (`LocalSnapshotStore` in dev).
- `.chunk_metadata_store(Arc<dyn ChunkMetadataStore>)` — typed chunk provenance (document version, snapshot URI, page/section/block anchors).
- `.evidence(Arc<dyn EvidenceResolver>)` — answers "show me the source" for any chunk, tree node, entity, or relation; served under `GET /evidence/chunk/:chunk_id`, `/evidence/tree-node/:node_id`, `/evidence/entity/:entity_id`, `/evidence/relation/:source_id/:relation_type/:target_id`.
- `.gc_worker(Arc<dyn GcWorker>)` — retention-policy garbage collection; requires Postgres-backed stores, so no example wires this in dev.

**Current wiring across the 6 examples:**

| Example | `version_store` | `snapshot_store` | `chunk_metadata_store` + `evidence` |
|---|---|---|---|
| Devforge | ✅ (`SqliteDocumentVersionStore`) | ✅ (`LocalSnapshotStore`) | — |
| Folio | ✅ | implicit default | ✅ (`DefaultEvidenceResolver`) |
| Meridian, Canopy, Helix, Vantage | not yet wired | not yet wired | not yet wired |

Devforge needed `version_store` to fix its document-count/list-documents endpoints (commit `48e42559`); Folio is the only example with the full evidence resolver wired, exercising `/evidence/*`. The other four examples don't wire any evidence-layer store yet — their `/api/v1/vector/collections/*/stats` and `/documents` endpoints will work but won't reflect document-level history, and `/evidence/*` will return nothing useful for their content.

---

## Example 1 — Standard + Static

**Devforge** · 25-person API-first SaaS startup
**Use case:** "Ask the Docs" — a search assistant embedded in their developer portal

### Content

~500 Markdown/HTML files: API reference, SDK guides, quickstarts, changelogs, internal runbooks.

### Why Standard ingestion

Documents are clean, short-section Markdown with no entity relationships worth graphing. Contextual enrichment adds latency and LLM cost without improving precision on well-structured technical prose. The team needs ingestion-to-search in minutes, not an enrichment pipeline.

### Why Static

Query patterns are predictable: developers either search for a method name or error code (BM25 wins) or ask a conceptual question (Vector wins). A fixed two-strategy set handles >95% of queries. QueryClassified adds a classifier call that's unnecessary when patterns are stable. ParallelFusion would spin up Graph and RAPTOR retrievers that return nothing — nothing was indexed there — and add latency for zero gain.

### Engine configuration

```rust
// Dev version store — persists version history across restarts so dedup works
// and document counts are accurate. Switch to PostgresDocumentVersionStore in production.
let version_store  = Arc::new(SqliteDocumentVersionStore::open("data/versions.db").await?);
let snapshot_store = Arc::new(LocalSnapshotStore::new("data/snapshots"));
let vector_store   = Arc::new(LanceDbStore::new("data/devforge.lance").await?);
let embedder       = Arc::new(OllamaProvider::new(&ollama, "nomic-embed-text", "nomic-embed-text"));

let engine = ArcanumEngineBuilder::new(config)
    .auth_secret(&secret)
    .vector_store(vector_store)
    .embedder(embedder)
    .version_store(version_store)
    .snapshot_store(snapshot_store)
    .build()
    .await?;
```

```toml
[retrieval]
orchestration_mode = "Static"
strategy_set       = ["Vector", "BM25"]
top_k              = 8
```

### Representative queries

| Query | Strategy that wins | Why |
|---|---|---|
| "invalid_api_key error" | BM25 | Exact phrase in error reference doc |
| "how do I authenticate with OAuth2?" | Vector | Semantic match across auth guide |
| "rate limit headers" | both | Overlap; first result set returned |

### Wrong choice cost

Full ingestion wastes 3–5× ingestion time extracting entities from API docs that have no useful graph relationships. ParallelFusion spins up Graph and RAPTOR retrievers that return empty results on every query.

---

## Example 2 — Standard + QueryClassified

**Meridian Consulting** · 150-person management consulting firm
**Use case:** "Policy Finder" — an internal Q&A assistant on the company intranet

### Content

~200 documents: HR handbook, benefits guides, parental leave policy, IT security SOPs, expense policies, code of conduct.

### Why Standard ingestion

Policy docs are well-structured prose with no entity relationships worth graphing and no long-document analytical queries that would justify RAPTOR. One-time ingestion with infrequent updates; fast time-to-value matters for an IT team that isn't running a dedicated ML pipeline.

### Why QueryClassified

Two structurally different query types coexist, each handled optimally by a different retriever:

- **Lookup queries** ("how many vacation days do I get in year 2?") — BM25 wins; the answer is a specific phrase in a policy table.
- **Conceptual queries** ("how does maternity leave affect my performance review cycle?") — Vector wins; the answer spans two separate policy sections with no shared phrasing.

Running both on every query doubles LLM embedding cost and latency at a company where IT cost is scrutinised. The classifier adds ~5ms to route correctly, saving a BM25 pass on semantic queries. The fallback (confidence below 0.7) runs both and returns the first result set.

### Engine configuration

```rust
let vector_store = Arc::new(LanceDbStore::new("data/meridian.lance").await?);
let embedder      = Arc::new(OllamaProvider::new(&ollama, "nomic-embed-text", "nomic-embed-text"));

let engine = ArcanumEngineBuilder::new(config)
    .auth_secret(&secret)
    .vector_store(vector_store)
    .embedder(embedder)
    .build()
    .await?;
```

```toml
[retrieval]
orchestration_mode              = "QueryClassified"
strategy_set                    = ["Vector", "BM25"]
classifier_confidence_threshold = 0.7
top_k                           = 6
```

### Representative queries

| Query | Routes to | Why |
|---|---|---|
| "PTO accrual rate for part-time employees" | BM25 | Exact phrase in benefits table |
| "can I take unpaid leave during an active project?" | Vector | Spans HR policy and project delivery sections |
| "401k match percentage" | BM25 | Specific term in benefits doc |
| "how does maternity leave affect my performance review?" | Vector | No shared phrase; answer requires cross-section reasoning |

### Wrong choice cost

Static with BM25-only fails all conceptual queries. ParallelFusion runs both strategies on every query — technically correct but at 150 employees × 30 queries/day the wasted Ollama compute per month is measurable on shared infrastructure.

---

## Example 3 — Standard + ParallelFusion

**Canopy Co.** · 80-person D2C outdoor gear brand
**Use case:** Site-wide search assistant for customers + internal tool for support agents

### Content

~3,500 documents: product spec sheets, user manuals, FAQs, return and warranty policies, support macros.

### Why Standard ingestion

Content is short and structured — contextual enrichment does not improve "waterproof rating: 20,000mm". Products do not have meaningful relational graph edges worth traversing. At ~3,500 documents, Full ingestion would be 3–5× more expensive for no measurable retrieval improvement.

### Why ParallelFusion

Two user populations with incompatible query styles share the same system:

- **Customers** use natural language: "best jacket for winter hiking", "what's the difference between GORE-TEX and eVent?"
- **Support agents** use exact identifiers: "SKU TN-4892 waterproof rating", "warranty terms model HK-221"

A classifier cannot reliably distinguish them. "Jacket for rainy weather" (semantic) and "SKU TN-4892 waterproof rating" (keyword) look textually similar at classification time. Misclassification frustrates one population.

The corpus is small enough (~3,500 docs) that running both strategies in parallel stays under 150ms P95. RRF handles the score scale difference between BM25 lexical ranks and ANN cosine distances without manual weight tuning.

### Engine configuration

```rust
let vector_store = Arc::new(LanceDbStore::new("data/canopy.lance").await?);
let embedder      = Arc::new(OllamaProvider::new(&ollama, "nomic-embed-text", "nomic-embed-text"));

let engine = ArcanumEngineBuilder::new(config)
    .auth_secret(&secret)
    .vector_store(vector_store)
    .embedder(embedder)
    .build()
    .await?;
```

```toml
[retrieval]
orchestration_mode = "ParallelFusion"
fusion_strategy    = "Rrf"
top_k              = 10
```

### Representative queries

| Query | Fusion result |
|---|---|
| "best tent for 3-season backpacking under $400" | Vector dominant; RRF promotes semantic matches |
| "SKU TN-4892 weight" | BM25 dominant; RRF promotes exact match |
| "waterproof jacket that breathes well" | Both contribute; RRF surfaces chunks where Vector and BM25 agree (highest-confidence intersection) |

### Wrong choice cost

QueryClassified fails because "GORE-TEX tent comparison" looks semantic but also contains exact-match keywords; misclassification frustrates customers. Static with Vector-only breaks support agents searching by SKU or model number.

---

## Example 4 — Full + QueryClassified

**Helix Labs** · 200-person drug discovery startup
**Use case:** "Research Copilot" — scientists query the company's accumulated research knowledge

### Content

~8,000 documents: research papers, clinical trial protocols, patent filings, regulatory submissions, compound assay results. Individual documents are 10–200 pages.

### Why Full ingestion

Three of the four extra Full stages each carry distinct, non-substitutable value:

**EntityExtract + GraphWrite:** Compounds, target proteins, biological pathways, and adverse events are deeply interconnected. "What else targets EGFR?" requires graph traversal across edges extracted from 12 different papers — a Vector search over text cannot reconstruct the relationship graph.

**RAPTORBuild:** Papers and protocols are long. RAPTOR L2 root nodes hold the full study summary; L1 nodes hold section summaries; L0 nodes hold passage-level content. "Summarize the adverse event profile across all Phase 2 JAK inhibitor trials" resolves at L1/L2; "what was the MTD in cohort 3?" resolves at L0.

**ContextEnrich:** Dense scientific text benefits from prepending paper title and section heading to each chunk before embedding. Without it, "She observed elevated ALT at day 14" is contextually ambiguous; with enrichment the chunk is anchored to its study, compound, and cohort.

In production, GLiNER handles entity extraction (compounds, genes, proteins) cheaply at scale and Claude Haiku handles context prefix generation and RAPTOR summaries. **Dev mode uses a single local Ollama model (`qwen2.5`) as the enricher for all three intents** — no external services required, at the cost of lower-quality entity extraction and summaries than the production split.

### Why QueryClassified

Scientists ask three structurally different query types, each best handled by a different strategy:

1. **Entity queries** — "Does Compound X bind to Receptor Y?" → entity mention detected → GraphRetriever traverses the compound→protein edge
2. **Analytical queries** — "Summarize adverse events across all Phase 2 JAK inhibitor trials" → RAPTOR traverses tree levels across multiple documents
3. **Semantic queries** — "CRISPR delivery mechanisms in neuronal tissue" → Vector + BM25 fallback

Running all in parallel (ParallelFusion) works but wastes GPU time on a 60–200ms RAPTOR tree traversal for simple entity-lookup queries. The classifier adds ~8ms and eliminates that cost on the majority of queries. The 0.7 confidence threshold falls back to Vector + BM25 when the query intent is ambiguous.

### Engine configuration

```rust
// Dev: in-memory graph + tree stores, single local-Ollama enricher — no external
// services required. Production: Neo4j, Postgres-backed tree store, GLiNER + Claude
// Haiku split via EnrichmentDispatcher (see BUILD.md).
let vector_store = Arc::new(LanceDbStore::new("data/helix.lance").await?);
let embedder     = Arc::new(OllamaProvider::new(&ollama, "nomic-embed-text", "nomic-embed-text"));
let enricher     = Arc::new(OllamaProvider::new(&ollama, "nomic-embed-text", "qwen2.5"));
let graph_store  = Arc::new(InMemoryGraphStore::new());
let tree_store   = Arc::new(InMemoryTreeStore::new());

let engine = ArcanumEngineBuilder::new(config)
    .auth_secret(&secret)
    .vector_store(vector_store)
    .embedder(embedder)
    .enricher(enricher)
    .graph_store(graph_store)
    .tree_store(tree_store)
    .build()
    .await?;
```

Production enricher (see `BUILD.md`):
```rust
let haiku  = Arc::new(AnthropicProvider::new(&claude_key, "claude-haiku-4-5-20251001"));
let gliner = Arc::new(GlinerProvider::new("http://localhost:8080"));
let enricher = Arc::new(
    EnrichmentDispatcher::new(haiku)
        .with_override(EnrichIntent::ExtractEntities, gliner)
);
```

```toml
[ingestion]
pipeline = "full"

[retrieval]
orchestration_mode              = "QueryClassified"
strategy_set                    = ["Vector", "BM25"]
classifier_confidence_threshold = 0.7
top_k                           = 12
```

### Representative queries

| Query | Routes to | Result |
|---|---|---|
| "compounds that inhibit EGFR" | Graph | Traverses compound→protein edges across 12 papers; 14 entity-linked chunks |
| "summarize adverse events across all Phase 2 JAK inhibitor trials" | RAPTOR | L1/L2 nodes spanning multiple trial protocols |
| "CRISPR delivery mechanisms in neuronal tissue" | Vector + BM25 (fallback) | Classifier confidence below threshold |
| "does compound 17g cross the blood-brain barrier?" | Graph | Entity lookup → assay result chunks |

### Wrong choice cost

Standard + QueryClassified misses the graph layer — "what else targets EGFR?" cannot traverse relationship-only edges; Vector returns only chunks that explicitly mention EGFR alongside another compound. Full + Static (Vector only) returns fragmented chunk-level results on analytical queries rather than RAPTOR summaries; scientists receive noise instead of synthesis.

### Known gap

Helix does not yet wire `.version_store()`. Document counts and history (`GET /api/v1/vector/collections/:name/documents`, `/stats`) won't reflect ingested documents the way Devforge's do — see [Evidence & versioning layer](#evidence--versioning-layer) above.

---

## Example 5 — Full + ParallelFusion

**Vantage Legal** · 120-person corporate legal team at a midsize private equity firm
**Use case:** "Contract Intelligence" — lawyers query the full contract portfolio to surface obligations, risks, and precedents

### Content

~4,000 documents: NDAs, vendor contracts, employment agreements, regulatory filings, internal compliance checklists, case law excerpts. Contracts range from 5 to 150 pages.

### Why Full ingestion

**EntityExtract + GraphWrite:** Contracts are entity-dense. Parties, obligations, dates, jurisdictions, penalty clauses, and indemnification terms are named entities with relationships. "Which contracts involve Acme Corp?" is a graph traversal across `Party:Acme_Corp → Contract` edges — including subsidiaries and DBAs captured as graph aliases at extraction time, which a text search would miss.

**RAPTORBuild:** Long contracts require cross-section synthesis. "What is our standard position on IP assignment in employment agreements?" requires reasoning across multiple clause types spread across dozens of pages. RAPTOR L2 root nodes hold the contract's structural summary; L1 nodes hold clause-group summaries; L0 nodes hold individual clause text.

**ContextEnrich:** "Section 12.3" is meaningless without knowing the document, parties, and governing law. Contextual enrichment prepends document type, party names, jurisdiction, and effective date to every chunk before embedding, making each vector self-interpreting.

As with Helix, dev mode runs a single local Ollama enricher (`qwen2.5`); production swaps in Claude Haiku + GLiNER via `EnrichmentDispatcher` (see `BUILD.md`).

### Why ParallelFusion

Legal queries are fundamentally unpredictable — lawyers jump between precision lookups and open-ended research in the same session:

- "indemnification clause" → BM25 exact match
- "what are our data residency obligations to EU vendors?" → Vector semantic
- "which contracts involve Acme Corp?" → Graph party entity traversal
- "what is our standard position on IP assignment?" → RAPTOR cross-section synthesis

A classifier fails here because legal query intent is ambiguous even to the lawyer writing the query. "Termination for convenience" is simultaneously an exact legal term (BM25), a concept spanning multiple clause types (Vector), and a graph node linking to related obligations (Graph). Misclassification has real professional consequences.

The corpus is small enough (4,000 docs) that all strategies complete within the 500ms per-strategy timeout. RRF handles score scale differences cleanly; partial results are valid if a graph traversal runs long on a complex query.

### Engine configuration

```rust
let vector_store = Arc::new(LanceDbStore::new("data/vantage.lance").await?);
let embedder     = Arc::new(OllamaProvider::new(&ollama, "nomic-embed-text", "nomic-embed-text"));
let enricher     = Arc::new(OllamaProvider::new(&ollama, "nomic-embed-text", "qwen2.5"));
let graph_store  = Arc::new(InMemoryGraphStore::new());
let tree_store   = Arc::new(InMemoryTreeStore::new());

let engine = ArcanumEngineBuilder::new(config)
    .auth_secret(&secret)
    .vector_store(vector_store)
    .embedder(embedder)
    .enricher(enricher)
    .graph_store(graph_store)
    .tree_store(tree_store)
    .build()
    .await?;
```

```toml
[ingestion]
pipeline = "full"

[retrieval]
orchestration_mode  = "ParallelFusion"
fusion_strategy     = "Rrf"
strategy_timeout_ms = 500
top_k               = 15
```

### Representative queries

| Query | Fusion contribution | Result |
|---|---|---|
| "indemnification cap for IT vendors" | BM25 (exact phrase) + Vector ("liability ceiling" neighbors) + Graph (Vendor entities with indemnification relations) | RRF surfaces the three-strategy intersection as highest confidence |
| "obligations that survive contract termination" | RAPTOR dominant (cross-section summary) + Vector + BM25 ("surviving obligations" phrase) | Synthesized clause-group view |
| "all contracts involving Acme Corp" | Graph dominant (Party entity) + Vector + BM25 ("Acme" raw text) | Includes subsidiaries via graph aliases missed by text search |

### Wrong choice cost

Full + QueryClassified fails because legal queries fool the classifier constantly — "surviving obligations after termination" looks like a keyword query but spans multiple conceptual clause types. Standard + ParallelFusion works but loses the graph layer; "which contracts involve Acme Corp as a subsidiary?" becomes a text search that misses graph edges captured at entity extraction time.

### Known gap

Like Helix, Vantage does not yet wire `.version_store()` or the evidence resolver — see [Evidence & versioning layer](#evidence--versioning-layer) above.

---

## Example 6 — Full + ParallelFusion

**Folio** · 60-person digital library platform serving schools and public libraries
**Use case:** Book search and discovery — patrons upload ePub/PDF books and query across the full collection, ranging from specific passage lookup to whole-book summarization

### Content

Tens of thousands of books in ePub and PDF format: fiction, non-fiction, reference works, multi-book series. A single book ranges from 50,000 to 400,000+ words.

### Why Full ingestion

This is the use case Full pipeline was designed for. All four extra stages carry distinct, non-substitutable value.

**EntityExtract + GraphWrite**

Books are entity-dense in a relational way that text search cannot replicate:
- Authors connect to multiple books and series
- Characters appear across multiple books in a series
- Places (real and fictional) link authors to themes and settings
- Series carry an explicit ordering that no single passage encodes

"All Agatha Christie novels featuring Hercule Poirot" is a graph traversal (`Author:Agatha_Christie → Book → Character:Hercule_Poirot`). "In what order do I read the Stormlight Archive?" lives in `Series:Stormlight_Archive → [Book, position]` subgraph. Neither query is answerable by text similarity search.

Entity types extracted: `Author`, `Character`, `Place` (real and fictional), `Series`, `Theme`, `Publisher`, `Genre`, `Year`.

**RAPTORBuild**

A book's content spans thousands of chunks. RAPTOR builds three levels:

- **L0 (leaf):** Individual passage chunks — paragraph or scene granularity
- **L1 (mid):** Chapter or section summaries — cluster of semantically related passages
- **L2 (root):** Whole-book summary — full arc, central themes, main argument

Without RAPTOR, "Summarize the plot of Moby Dick" returns 40 random chunks about whales. With RAPTOR, the L2 root node answers the question directly. "What happens in Chapter 5 of The Hobbit?" resolves at L1. "What does Bilbo say when he finds the ring?" resolves at L0. All three levels participate in every query; RAPTOR weights leaf results for specific queries and root results for broad analytical queries.

**ContextEnrich**

Raw book chunks are deeply context-dependent: "She looked at him and said nothing" is meaningless without knowing it is from *Pride and Prejudice*, Chapter 34, narrating Elizabeth's reaction to Darcy's first proposal. Contextual enrichment prepends `[Book: Pride and Prejudice | Author: Jane Austen | Chapter: 34]` to every chunk before embedding, anchoring each vector in its book context.

Dev mode runs a single local Ollama enricher (`qwen2.5`); production splits entity extraction to SpaCy and context-prefix/summary generation to Claude Haiku via `EnrichmentDispatcher`.

**Evidence layer:** Folio is the only example that wires the full evidence stack — `.version_store()`, `.chunk_metadata_store()`, and `.evidence()` with a `DefaultEvidenceResolver`. For a library this matters most: "show me the source" for a generated summary needs to resolve back to the exact book, chapter, and passage. See [Evidence & versioning layer](#evidence--versioning-layer) above.

### Why ParallelFusion

Library queries are the most unpredictable workload in this example set. A patron in a single session might ask:

1. A specific passage: "What does Atticus say to Scout about real courage?"
2. A series question: "In what order should I read the Mistborn series?"
3. A high-level summary: "What is the central argument of Thinking Fast and Slow?"
4. A cross-collection discovery: "Which books in the library deal with grief and loss?"
5. An author/universe query: "All books by Ursula K. Le Guin set in the Hainish Cycle"

No classifier reliably routes these. Query 1 is simultaneously a character lookup (Graph) and a passage search (Vector + BM25). Query 5 is both a graph traversal and a metadata filter. Query 4 is thematic (RAPTOR + Vector). The queries look textually similar in many cases; misclassification produces conspicuously wrong answers that frustrate patrons.

ParallelFusion runs all four enabled strategies concurrently within a 600ms timeout. RRF handles score scale differences between BM25 lexical ranks, ANN cosine scores, and RAPTOR level-weighted scores without manual weight tuning. Partial results are valid if a strategy times out on an unusually complex graph traversal.

### Engine configuration

```rust
let vector_store         = Arc::new(LanceDbStore::new("data/folio.lance").await?);
let embedder             = Arc::new(OllamaProvider::new(&ollama, "nomic-embed-text", "nomic-embed-text"));
let enricher             = Arc::new(OllamaProvider::new(&ollama, "nomic-embed-text", "qwen2.5"));
let graph_store          = Arc::new(InMemoryGraphStore::new());
let tree_store           = Arc::new(InMemoryTreeStore::new());
let version_store        = Arc::new(SqliteDocumentVersionStore::open("data/versions.db").await?);
let chunk_metadata_store = Arc::new(InMemoryChunkMetadataStore::new());

let evidence_resolver = Arc::new(DefaultEvidenceResolver::new(
    chunk_metadata_store.clone(),
    version_store.clone(),
    tree_store.clone(),
    graph_store.clone(),
));

let engine = ArcanumEngineBuilder::new(config)
    .auth_secret(&secret)
    .vector_store(vector_store)
    .embedder(embedder)
    .enricher(enricher)
    .graph_store(graph_store)
    .tree_store(tree_store)
    .version_store(version_store)
    .chunk_metadata_store(chunk_metadata_store)
    .evidence(evidence_resolver)
    // GC worker requires Postgres (retention-policy bookkeeping lives in
    // document_versions). Not wired in this in-memory dev example — production:
    //   .gc_worker(Arc::new(PostgresGcWorker::new(
    //       &db_url, version_store, snapshot_store, vector_store,
    //       tree_store, graph_store, chunk_metadata_store,
    //   ).await?))
    .build()
    .await?;
```

Patron-submitted ingestion (via the library's upload endpoint). `IngestRequest` now also
carries `content`/`mime_hint` so the REST `/api/v1/upload` route can post raw file bytes
directly instead of only a `source_uri` the server has to fetch itself:
```rust
engine.ingestion.ingest(IngestRequest {
    source_uri: "s3://folio-uploads/patron-42/the-hobbit.epub".into(),
    collection_id: CollectionId("public_library".into()),
    pipeline_template: Some("full".into()),
    force: false,
    content: None,
    mime_hint: None,
}, "operator").await?;
```

Three RAPTOR levels are needed for full books:
```toml
[ingestion]
pipeline = "full"

[tree]
raptor_max_depth    = 3
raptor_cluster_size = 10

[retrieval]
orchestration_mode  = "ParallelFusion"
fusion_strategy     = "Rrf"
strategy_timeout_ms = 600
top_k               = 15
```

### Representative queries across the full spectrum

**Very specific — passage-level (L0 dominant)**

| Query | Strategies | Result |
|---|---|---|
| "What does Atticus Finch say to Scout about real courage?" | Vector (semantic passage) + BM25 (name match) | L0 leaf chunk from the morphine speech in Chapter 11 |
| "In which chapter does Frodo first encounter the Nazgûl?" | Vector + RAPTOR L1 (chapter summary names the event) | L1 chapter node "A Knife in the Dark" + L0 passage |
| "The exact opening line of Anna Karenina" | BM25 dominant | First chunk of the novel |
| "What does Jay Gatsby say the first time he meets Daisy?" | Vector (character + emotional context) + Graph (Character:Gatsby + Character:Daisy co-occurrence → Chapter 5) | L0 passage from the reunion scene |

**Series, authors, characters — graph-dominant**

| Query | Strategies | Result |
|---|---|---|
| "All books by Ursula K. Le Guin in the Hainish Cycle" | Graph dominant (`Author:Le_Guin → Series:Hainish_Cycle → Books`) | Structured book list with collection links |
| "In what order should I read the Mistborn series?" | Graph dominant (`Series:Mistborn → [Book, position]`) | Ordered series list |
| "Which Agatha Christie books feature both Poirot and Hastings?" | Graph dominant (character co-occurrence edges across books) | Filtered book list |
| "What other books share a universe with The Left Hand of Darkness?" | Graph (`Book → Series/Universe → related Books`) + Vector | Hainish Cycle books + thematically similar results |

**High-level summarization — RAPTOR-dominant**

| Query | Strategies | Result |
|---|---|---|
| "Summarize the plot of Moby Dick" | RAPTOR L2 root dominant | Full-arc summary; not 40 random whale chunks |
| "What are the major themes in Crime and Punishment?" | RAPTOR L2 + Vector (thematic chunks) | Root summary + supporting thematic passages |
| "What is the central argument of Thinking Fast and Slow?" | RAPTOR L2 + L1 (chapter summaries) | Book-level argument + key chapter points |
| "Compare how Hemingway and Fitzgerald use symbolism" | RAPTOR L2 per author + Graph (Author entities) | Cross-book synthesis from both authors' root summaries |

**Cross-collection discovery — fusion earns its keep**

| Query | Strategies | Result |
|---|---|---|
| "Books in the library about grief and loss" | Vector dominant (thematic similarity) + RAPTOR L2 (topic in root summaries) | Fiction and non-fiction across genres; no single keyword surfaces all |
| "Dystopian novels set after a pandemic" | Vector (semantic theme) + Graph (`Theme:dystopia` + `Theme:pandemic` → Books) | RRF surfaces books where both strategies agree |
| "Books set in 19th century Paris" | Graph (`Place:Paris` + `Year:19th_century` → Books) + Vector (semantic setting) | Structured + semantic coverage |

### Wrong choice cost

**Standard + ParallelFusion:** "Summarize Moby Dick" returns 40 fragmented passage chunks — no whole-book summary exists because RAPTOR was never built. "All books by Melville" becomes a text search over documents containing "Melville" — works for his own books but misses bibliography entries, biographical works, and books that discuss him by relation rather than by name.

**Full + QueryClassified:** "Jay Gatsby party quote" is simultaneously a character lookup (Graph) and a passage search (Vector); the classifier picks one and is wrong half the time. "Books about grief" looks semantic to the classifier but RAPTOR L2 root summaries carry the clearest thematic signal; routing to Vector-only misses those. Series ordering queries ("what Mistborn books are there") occasionally look semantic to the classifier and get routed to Vector instead of Graph, returning thematic passages instead of the ordered series list.

**Standard + Static (Vector + BM25 only):** Author, series, character, and universe queries all degrade to text search over raw content. All summarization queries return fragmented chunk collages instead of synthesized answers.

---

*Arcanum Usage Examples — produced via brainstorming session, 2026-06-01 — updated 2026-06-16 against the shipped `examples/` implementations and the evidence/versioning layer.*
