# arcanum-core + arcanum-models

## Purpose

`arcanum-core` is the foundation crate of the workspace: the shared domain
types (`Chunk`, `Document`, `Query`, `Entity`, `TreeNode`, evidence and
provenance types), the error taxonomy (`ArcanumError`), the layered
`ArcanumConfig`/`LiveConfig`, and — most importantly — the port traits
(`VectorStore`, `GraphStore`, `TreeStore`, `LexicalIndex`, `GraphPlanner`,
`DocumentLoader`, `Chunker`, `Embedder`, `TextEnricher`, `Retriever`,
`EvidenceResolver`, `DocumentVersionStore`, `SnapshotStore`, and more) that
every storage backend, model provider, and pipeline stage in the rest of the
workspace is written against. It has zero path dependencies on other
workspace crates and makes no network calls — every other crate depends on
it, it depends on none of them.

`arcanum-models` is the model-provider crate: concrete `Embedder` and
`TextEnricher` implementations for nine backends (Ollama, OpenAI, Anthropic,
HuggingFace TEI, BGE/E5, Mistral, LLM2Vec, GLiNER, spaCy), plus the
cross-cutting infrastructure around them — parallelism routing, per-intent
enrichment dispatch, a Redis-backed embedding cache, and provider health
tracking. It is covered on this page because it has no domain types of its
own: its entire public surface exists to implement `arcanum-core`'s
`Embedder`/`TextEnricher` ports.

## Position in the System

`arcanum-core` consumes nothing else in the workspace. `arcanum-models`
consumes only `arcanum-core` (it implements `Embedder`/`TextEnricher` and
uses `CacheInvalidator` for cache invalidation).

Both are consumed by every other layer:
- [Storage](storage.md) — `arcanum-vector`/`arcanum-graph`/`arcanum-tree`
  implement `VectorStore`/`GraphStore`/`TreeStore`; `arcanum-vector`'s BM25
  index implements `LexicalIndex`, `arcanum-graph` implements `GraphPlanner`.
- [Ingestion](ingestion.md) — `arcanum-ingestion` implements `DocumentLoader`,
  `Preprocessor`, `Chunker`.
- [Pipeline](pipeline.md) — `arcanum-pipeline` runs DAG stages against the
  `Chunker`/`ProgressEmitter`/`CacheInvalidator` ports and calls
  `arcanum-models` providers directly for embedding and enrichment.
- [Retrieval](retrieval.md) — `arcanum-retrieval` implements
  `Retriever`/`Reranker`/`Evaluator` and consumes `LexicalIndex`/
  `GraphPlanner` trait objects rather than concrete storage crates.
- [Evidence](evidence.md) — `arcanum-evidence` implements `EvidenceResolver`/
  `GcWorker`/`ChunkMetadataStore` against the `DocumentVersionStore`/
  `SnapshotStore` types defined here.
- [Evaluation](evaluation.md) — `arcanum-eval` implements `Evaluator`.
- [Engine](engine.md) — `arcanum-engine` composes every port above, plus
  `arcanum-models` providers, behind `ArcanumEngine`'s builder.

## Architecture

```mermaid
classDiagram
    class VectorStore { <<trait>> upsert() search() delete_by_source_uri() }
    class GraphStore { <<trait>> upsert_entities() query() get_relation() }
    class TreeStore { <<trait>> insert_node() get_children() get_by_id() }
    class LexicalIndex { <<trait>> search() }
    class GraphPlanner { <<trait>> plan_entities() }
    class DocumentLoader { <<trait>> load() supports() }
    class Chunker { <<trait>> chunk() }
    class Embedder { <<trait>> embed() dimension() }
    class TextEnricher { <<trait>> enrich() }
    class Retriever { <<trait>> retrieve() strategy() }
    class EvidenceResolver { <<trait>> resolve_chunk() resolve_entity() }
    class DocumentVersionStore { <<trait>> get_latest() add_version() }
    class SnapshotStore { <<trait>> store() fetch_raw() }

    class EmbeddingParallelismRouter
    class EnrichmentDispatcher
    class OllamaProvider
    class OpenAiProvider
    class MistralProvider
    class AnthropicProvider
    class GlinerProvider
    class BgeProvider

    EmbeddingParallelismRouter ..|> Embedder : wraps N providers, round-robin
    EnrichmentDispatcher ..|> TextEnricher : routes by EnrichIntent
    OllamaProvider ..|> Embedder
    OllamaProvider ..|> TextEnricher
    OpenAiProvider ..|> Embedder
    OpenAiProvider ..|> TextEnricher
    MistralProvider ..|> Embedder
    MistralProvider ..|> TextEnricher
    AnthropicProvider ..|> TextEnricher
    GlinerProvider ..|> TextEnricher
    BgeProvider ..|> Embedder
```

`arcanum-core/src/traits/` holds one file per port family: `store.rs`
(`VectorStore`, `GraphStore`, `TreeStore`, `SecretStore`, plus the
free functions `relation_identity_key`, `relation_touches_removed_entity`,
and `merge_relation` that every `GraphStore` implementation shares rather
than reimplementing), `lexical_index.rs`, `graph_planner.rs`, `ingestion.rs`
(`DocumentLoader`, `Preprocessor`, `Chunker`, plus the `Source` enum and its
`from_uri`/`display_uri` parsing), `model.rs` (`Embedder`, `TextEnricher`,
and `IngestionDepsOverrideResolver`), `retrieval.rs` (`Retriever`,
`Reranker`, `Evaluator`), `evidence.rs`, `versioning.rs`, `snapshot.rs`,
`cache.rs` (`CacheInvalidator`), and `progress.rs` (`ProgressEmitter`).
`arcanum-core/src/types/` holds the domain data these traits move: `Chunk`/
`IndexedChunk`/`RetrievedChunk` (`document.rs`), `Entity`/`Relation`
(`graph.rs`), `TreeNode` (`tree.rs`), `Query`/`MetadataFilter` (`query.rs`),
`ChunkProvenance`/`DocumentVersion`/`VersioningPolicy` (`provenance.rs`),
`ProofChain`/`ChunkMetadataRecord`/`GcReport` (`evidence.rs`), and
`PerBackendChunkConfig`/`PerBackendChunkers` (`chunk_config.rs`).
`ArcanumConfig` (`config.rs`) aggregates one config struct per subsystem
(`GlobalConfig`, `IngestionConfig`, `EmbeddingConfig`, `EnrichmentConfig`,
`StorageConfig`, `RetrievalConfig`, `EvalConfig`, `AdminConfig`,
`ServerConfig`) and `LiveConfig` wraps it in `Arc<RwLock<_>>` for runtime
patching.

`arcanum-models` implements no core types — every provider struct
(`OllamaProvider`, `OpenAiProvider`, `AnthropicProvider`,
`HuggingFaceTeiProvider`, `BgeProvider`, `MistralProvider`,
`Llm2VecProvider`, `GlinerProvider`, `SpacyProvider`) implements `Embedder`,
`TextEnricher`, or both, per the capability matrix in Key Decisions.
`EmbeddingParallelismRouter` and `EnrichmentDispatcher` also implement
`Embedder`/`TextEnricher` respectively, so a caller holding `Arc<dyn
Embedder>` cannot tell whether it's talking to one provider or a composed
router. `EmbeddingCache` (Redis) and `ProviderHealthMonitor` are standalone
structs, not trait implementations — see Implementation Notes for their
wiring status.

## Runtime Flows

**1. Embedding/generation request through arcanum-models**
1. A caller holds `Arc<dyn Embedder>` (wired by whatever composition root —
   an example app or `arcanum-engine`'s builder — constructed it).
2. If that trait object is an `EmbeddingParallelismRouter`,
   `EmbeddingParallelismRouter::embed` picks the next provider by
   incrementing an `AtomicUsize` counter modulo `providers.len()` and
   forwards the call.
3. The call lands on a concrete provider's `embed()` —
   `OllamaProvider`, `OpenAiProvider`, `MistralProvider`,
   `Llm2VecProvider`, `BgeProvider`, or `HuggingFaceTeiProvider` (the only
   six that implement `Embedder`).
4. The provider issues an HTTP request via `reqwest::Client`, records
   `arcanum_model_calls_total`/`arcanum_model_call_duration_seconds`
   metrics, and maps transport/parse failures to `ArcanumError::Embedding`.
5. Enrichment requests follow the same shape through `Arc<dyn
   TextEnricher>`: `EnrichmentDispatcher::enrich` computes an `intent_key`
   from the request's `EnrichIntent`, looks it up in its `overrides` map,
   and falls back to a `default` provider if there's no override —
   e.g. `GlinerProvider` for `ExtractEntities`, `AnthropicProvider` for
   `ContextPrefix`.

**2. The hexagonal boundary, via `VectorStore`**
1. `arcanum-core::traits::store::VectorStore` defines the port: `upsert`,
   `search`, `delete`, `collection_exists`, `delete_by_source_uri` are
   required; `list_collections`, `create_collection`, `count_documents`,
   `delete_collection` have default no-op bodies backends override as
   needed.
2. Concrete backends in `arcanum-vector` (documented on storage.md)
   implement `VectorStore` against LanceDB or PgVector.
3. `arcanum-engine`'s builder wires one implementation behind `Arc<dyn
   VectorStore>` and exposes it as a public field on `ArcanumEngine`.
4. `arcanum-pipeline`'s write stage and `arcanum-retrieval`'s vector
   strategy consume that `Arc<dyn VectorStore>` — neither names a concrete
   backend type, so switching LanceDB for PgVector is a builder change.

**3. Config loading and hot reload**
1. `ArcanumConfig::merged(file_path)` layers `Self::default()` →
   `from_file()` (TOML or YAML, chosen by extension) → `from_env()`
   (`ARCANUM_*` variables), each layer overriding the previous one for the
   fields it recognizes.
2. `ArcanumConfig::validate()` enforces one hard invariant before startup:
   `RuntimeMode::Production`/`Enterprise` reject `MetadataBackend::Sqlite`.
3. The validated config is wrapped in `LiveConfig`
   (`Arc<RwLock<ArcanumConfig>>`); its `update()` lets the admin API patch
   fields at runtime without a process restart.

## Key Decisions

### Evidence/provenance types and traits placed in arcanum-core, not arcanum-evidence
- **Decision** — `ChunkProvenance`, `DocumentVersion`, `VersioningPolicy`,
  `SnapshotLocation`, `EvidenceKind`, `ProofNode`, `RawSourceRef`,
  `ProofChain`, `ChunkMetadataRecord`, `GcReport` (types), and
  `DocumentVersionStore`, `SnapshotStore`, `EvidenceResolver`,
  `ChunkMetadataStore`, `GcWorker` (traits) all live in `arcanum-core`;
  only the concrete `DefaultEvidenceResolver` implementation lives in the
  new `arcanum-evidence` crate (the other concrete `GcWorker`,
  `PostgresGcWorker`, ended up in `arcanum-ingestion` alongside its
  `ChunkRegistry`/`document_registry` code, not in `arcanum-evidence`).
- **Context** — PR #44 (Evidence Phase 1) added `ChunkProvenance`,
  `DocumentVersion`, and the `DocumentVersionStore`/`SnapshotStore` traits
  to `arcanum-core`; PR #45 (Evidence Phase 2, Task 1–2) added the
  remaining evidence types and `EvidenceResolver`/`ChunkMetadataStore`/
  `GcWorker` to `arcanum-core`, then (Task 8) placed
  `DefaultEvidenceResolver` in a new `arcanum-evidence` crate.
- **Alternatives rejected** — No PR or design doc records a rationale for
  a types-and-traits-in-arcanum-evidence split; observed current state:
  the placement mirrors the existing pattern for `VectorStore`/
  `GraphStore`/`TreeStore`, where the port lives beside the other core
  ports and only the concrete adapter gets its own crate — though that
  pattern isn't fully carried through, since `PostgresGcWorker` landed in
  `arcanum-ingestion` rather than `arcanum-evidence` (see Implementation
  Notes).
- **Consequences** — any crate that only constructs or reads evidence
  types (e.g. `arcanum-pipeline`'s snapshot/chunk-metadata write stages)
  depends on `arcanum-core` alone, not on `arcanum-evidence`.
- **Ref** — 2026-06-16, PR #44 and PR #45.

### Per-backend chunking via PerBackendChunkConfig/PerBackendChunkers
- **Decision** — chunking configuration and runtime chunkers are keyed per
  storage backend: `ChunkStrategyConfig` (strategy name + JSON params),
  `PerBackendChunkConfig` (`vector` required, `graph`/`tree` optional), and
  the runtime `PerBackendChunkers` (`Arc<dyn Chunker>` × 3) replace a single
  chunker used for every backend.
- **Context** — PR #37 replaced a hardcoded `FixedSizeChunker` with a
  `ChunkRegistry`-driven, per-backend chunker resolution;
  `IngestionConfig::default().chunking` equals
  `PerBackendChunkConfig::default()` (`fixed`, 512/64 overlap for vector;
  `None` for graph and tree).
- **Alternatives rejected** — the PR body frames this as a direct
  replacement of the prior single-chunker design rather than a choice among
  live alternatives; no other option is recorded as considered and
  rejected.
- **Consequences** — one ingestion run can chunk the same source document
  differently for vector, graph, and tree storage; collection-level
  overrides layer on top via `arcanum-engine`'s `CollectionInfo`.
- **Ref** — 2026-06-07, PR #37.

### delete_by_source_uri and source_uri added to VectorStore/GraphStore/TreeStore for dedup cleanup
- **Decision** — PR #29 added `delete_by_source_uri(collection,
  source_uri)` to all three storage-port traits, and `source_uri` fields to
  `Entity` and `TreeNode`, so a changed re-ingest can atomically remove one
  document's chunks/entities/tree-nodes from every backend before writing
  the new version.
- **Context** — the in-memory `DocumentHashTracker` was replaced by a
  persistent `DocumentRegistry`, with `Dedup`/`Cleanup` pipeline stages;
  `Cleanup` needed to remove exactly one document's data per store without
  touching the rest of the collection.
- **Alternatives rejected** — PR #30's fix #1 records that an unguarded
  `delete_by_source_uri("")` would mass-delete every chunk in a collection;
  rather than trust callers never to pass an empty string, an explicit
  early-return guard was added to all six store implementations. PR #30
  also replaced ad hoc `deregister()` with a CAS-based `try_set_replacing`
  transition to close a concurrent-worker race on the registry.
- **Consequences** — every `VectorStore`/`GraphStore`/`TreeStore`
  implementation, present or future, must guard the empty-`source_uri`
  case itself — it is not enforced by the trait signature. **Superseded** —
  PR #44 (Evidence Phase 1) later replaced `DocumentRegistry` and its
  CAS-based `try_set_replacing`/`deregister()` transition with
  `DocumentVersionStore`; `delete_by_source_uri` itself is unaffected and
  is still called by the current cleanup stage (see Implementation Notes).
- **Ref** — 2026-06-04, PR #29 and PR #30; superseded by 2026-06-16, PR #44.

### LexicalIndex and GraphPlanner extracted so arcanum-retrieval depends only on arcanum-core
- **Decision** — introduced `LexicalIndex` (`async search(collection_id,
  query, top_k)`) and `GraphPlanner` (`async plan_entities(query)`) traits
  in `arcanum-core`, and changed `Bm25Retriever` to hold `Arc<dyn
  LexicalIndex>` instead of a concrete `arcanum_vector::Bm25Index`.
- **Context** — before this commit, `arcanum-retrieval`'s `Cargo.toml`
  depended directly on `arcanum-vector` and `arcanum-graph` as regular
  dependencies (confirmed in the commit's diff to
  `arcanum-retrieval/Cargo.toml`), so the retrieval crate could only build
  against those two concrete storage crates.
- **Alternatives rejected** — No PR or design doc records a rationale for
  the specific trait shapes chosen; observed current state: the commit
  moved `arcanum-vector` and `arcanum-graph` from `arcanum-retrieval`'s
  `[dependencies]` to `[dev-dependencies]` — still linked for the crate's
  own tests, no longer part of its public build graph.
- **Consequences** — `arcanum-retrieval`'s non-test build depends only on
  `arcanum-core`; a future lexical or graph-planning backend can be
  swapped in by implementing `LexicalIndex`/`GraphPlanner` without
  touching `arcanum-retrieval`.
- **Ref** — 2026-06-01, commit `976c9458`.

### Provider capability is split across two traits, and enrichment routes independently from embedding
- **Decision** — `Embedder` (`embed`/`dimension`) and `TextEnricher`
  (`enrich`) are separate traits; each provider implements only the
  capability it actually has. Ollama, OpenAI, Mistral, and LLM2Vec
  implement both; HuggingFace TEI and BGE implement only `Embedder`;
  Anthropic, GLiNER, and spaCy implement only `TextEnricher` — `GlinerProvider::enrich`
  and `SpacyProvider::enrich` both return `ArcanumError::Enrichment` for
  any `EnrichIntent` other than `ExtractEntities`.
- **Context** — the architecture design doc (untracked) states the goal
  directly: "a single Ollama deployment with Qwen2.5 satisfies both
  Embedder and TextEnricher — zero external dependencies for a full local
  setup," and documents the same provider-capability matrix found in
  source. It also states "Different TextEnricher intents can route to
  different providers. Example: GLiNER for ExtractEntities (fast, cheap),
  Claude for ContextPrefix (high quality)" — exactly what
  `EnrichmentDispatcher::with_override` implements.
- **Alternatives rejected** — No PR or design doc records a rationale for
  rejecting one combined provider trait; observed current state: the split
  lets embedding-only services (TEI, BGE) and enrichment-only services
  (Claude, GLiNER, spaCy) each implement only what they support, instead
  of stubbing the other capability with a runtime error.
- **Consequences** — a provider needing both capabilities is one struct
  implementing two traits (`OllamaProvider`); per-intent routing needs
  `EnrichmentDispatcher`, a different composition than the round-robin
  `EmbeddingParallelismRouter` used for embedding, because embedding has
  no intent to key on.
- **Ref** — 2026-05-28, commit `447321f7` (traits); 2026-05-29, commit
  `de5ad7c2` (dispatcher/router); 2026-05-30, commits
  `54685d22` and `e29d62c2` (V5 provider additions); the architecture
  design doc (untracked).

## Implementation Notes

- **Unwired infra (debt).** `EmbeddingCache` (Redis) and
  `ProviderHealthMonitor` are fully implemented and unit-tested in
  `arcanum-models` but have no callers anywhere else in the workspace —
  neither is constructed by any provider, by `arcanum-engine`, or by any
  example app.
- **Unwired composition (debt).** `EmbeddingParallelismRouter` is not
  referenced outside `arcanum-models` at all. `EnrichmentDispatcher`
  appears in the three example apps
  (`folio-library-search`, `helix-research-copilot`,
  `vantage-contract-intel`) only inside a `// Production:
  EnrichmentDispatcher::new(...)` comment — each example actually
  constructs a bare `OllamaProvider` for both embedding and enrichment.
- **Dead config fields (debt).** `EmbeddingConfig.cache_enabled` and
  `RetrievalConfig.query_cache_enabled` are read nowhere in the workspace;
  setting either in a config file currently has no effect.
- `relation_identity_key`, `relation_touches_removed_entity`, and
  `merge_relation` in `traits::store` are free functions, not trait
  methods, called by `InMemoryGraphStore` (`arcanum-graph/src/lib.rs`) and
  `SledGraphStore` (`arcanum-graph/src/sled_store.rs`) to mirror
  `Neo4jStore`'s `MERGE`-by-`(source, relation_type, target)` and
  `DETACH DELETE` semantics, which `Neo4jStore`
  (`arcanum-graph/src/neo4j_store.rs`) implements independently in Cypher
  and never calls these functions — a fix to `merge_relation` changes the
  in-memory and Sled backends' behavior but has no effect on the Neo4j
  backend.
- **Superseded dedup mechanism (drift).** `DocumentRegistry` and its
  `try_set_replacing`/`deregister()` transition (Key Decisions, PR #29/#30)
  no longer exist in source — `arcanum-ingestion/src/document_registry.rs`
  is now a two-line stub. The current `make_dedup_stage`/
  `make_cleanup_stage` (`arcanum-pipeline/src/stages.rs`) take `Arc<dyn
  DocumentVersionStore>`, calling `get_latest()` to decide skip/replace;
  the `supersede_active(document_id)` call (an `Active` → `Superseded`
  `VersionStatus` transition) that replaces the old registry's CAS state
  actually fires from `make_snapshot_stage` under
  `VersioningPolicy::Replace` — `make_cleanup_stage`'s own supersede guard
  is unreachable (see [Pipeline](pipeline.md)).
- `IngestionDepsOverrideResolver` inverts the usual direction: the trait is
  defined in `arcanum-core` but implemented by `arcanum-engine` and called
  by each `arcanum-pipeline` worker — the consumer (pipeline) sits below
  the implementer (engine) in the crate DAG, opposite to the `VectorStore`
  pattern.
- **Inconsistent crate placement.** `PostgresGcWorker` (a `GcWorker`
  implementation, PR #45 Task 9) lives in `arcanum-ingestion/src/gc.rs`,
  not in `arcanum-evidence` alongside `DefaultEvidenceResolver` — the two
  concrete evidence-layer implementations that PR #45 added ended up in
  different crates.
- `NoOpDocumentVersionStore` treats every document as new (no dedup); the
  Evidence Phase 1 PR (#44) changed the engine builder to require an
  explicit `version_store` rather than silently falling back to this type,
  specifically to prevent dedup from being disabled unnoticed.
- `ArcanumConfig::validate()`'s SQLite-in-production rejection and
  `DoclingConfig`'s HTTP/CLI backend field checks are the only two
  cross-field validations currently enforced.

## Source Anchors

- `arcanum-core/src/lib.rs`
- `arcanum-core/src/error.rs`
- `arcanum-core/src/config.rs`
- `arcanum-core/src/types/` (module)
- `arcanum-core/src/traits/` (module)
- `arcanum-models/src/` (crate)

## Related Pages

- [Storage](storage.md)
- [Ingestion](ingestion.md)
- [Pipeline](pipeline.md)
- [Retrieval](retrieval.md)
- [Evidence](evidence.md)
- [Engine](engine.md)
- [Evaluation](evaluation.md)
- [Interfaces](interfaces.md)
