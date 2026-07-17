# arcanum-pipeline

## Purpose

`arcanum-pipeline` turns one `IngestionTask` into a completed (or
deliberately skipped) ingest. `dag.rs`/`stages.rs` define a DAG of named
stages, `executor.rs`'s `DagExecutor` runs that DAG in dependency order,
`registry.rs`'s `ArcanumPipelineRegistry` selects one of five DAG-building
templates (`templates/`) by name, and `worker.rs`'s `IngestionWorker`
pulls tasks off a queue, resolves per-collection dependency overrides, and
drives each task through the DAG with a retry-and-requeue loop. This page
also covers `arcanum-middleware`, a small crate of three reliability
primitives — `BoundedQueue`, `RetryPolicy`, `CircuitBreaker` — that back
this crate's queueing, retry, and failure-isolation behavior;
`ArcanumEngineBuilder::build` (`arcanum-engine/src/engine.rs`) is where
all three are actually constructed and configured, then handed to this
crate. Splitting DAG orchestration and
stage sequencing into their own crate keeps `arcanum-engine` a
composition root rather than a place where ingestion control flow lives.

## Position in the System

`arcanum-pipeline` consumes [Core](core.md) — `arcanum_core::traits`
(`DocumentVersionStore`, `SnapshotStore`, `ChunkMetadataStore`,
`VectorStore`/`GraphStore`/`TreeStore`, `Chunker`, `Embedder`,
`TextEnricher`, `Preprocessor`, `ProgressEmitter`,
`IngestionDepsOverrideResolver`) and `arcanum_core::types`
(`IngestionTask`, `IngestionReport`, `PerBackendChunkers`,
`ShadowContext`, `DocumentVersion`, `ChunkMetadataRecord`) — plus
[Ingestion](ingestion.md)'s concrete `LoaderRegistry`, `MimeDetector`,
`ContextEnricher`, `EntityExtractor` types (not trait objects), and
`arcanum-tree`'s concrete `RaptorBuilder`. It also consumes
`arcanum-middleware`'s `BoundedQueue`, `RetryPolicy`, `CircuitBreaker` as
concrete types, not trait objects — [Engine](engine.md) constructs them
(`CircuitBreaker::new("embedding", ...)`, `BoundedQueue::new("ingestion",
...)`, `RetryPolicy::new` in `ArcanumEngineBuilder::build`) and
`IngestionService` pushes onto the same `BoundedQueue`; this crate is
where they gate stage execution.

- [Engine](engine.md) — `ArcanumEngineBuilder` assembles the shared
  `PipelineDeps` and `ArcanumPipelineRegistry`, then builds
  `IngestionWorker`s from them. `EngineIngestionDepsResolver` implements
  `IngestionDepsOverrideResolver` and is attached via
  `IngestionWorker::with_resolver`, so per-job dependency resolution is
  engine-owned even though the trait is defined in Core and called from
  here (see Core's "`IngestionDepsOverrideResolver` inverts the usual
  direction" note).
- [Storage](storage.md) — `arcanum-vector`/`arcanum-graph`/`arcanum-tree`
  concrete stores reach this crate only through the
  `VectorStore`/`GraphStore`/`TreeStore` trait objects in `PipelineDeps`,
  except `RaptorBuilder`, which `make_raptor_build_stage` constructs
  directly from the concrete `arcanum-tree` type.
- [Evidence](evidence.md) — the `snapshot`, `vector_write`, and
  `register_version` stages write through the `SnapshotStore`/
  `ChunkMetadataStore`/`DocumentVersionStore` trait objects that
  `arcanum-evidence`'s concrete stores back; this crate has no direct
  dependency on `arcanum-evidence`.

## Architecture

```mermaid
classDiagram
    class PipelineDAG
    class PipelineStage
    class DagExecutor
    class ArcanumPipelineRegistry
    class PipelineDeps
    class IngestionState
    class IngestionWorker
    class BoundedQueue
    class RetryPolicy
    class CircuitBreaker
    class IngestionDepsOverrideResolver { <<trait>> }

    PipelineDAG o-- PipelineStage : stages
    ArcanumPipelineRegistry --> PipelineDAG : build(name, state, deps)
    DagExecutor --> PipelineDAG : execute(dag, ctx)
    PipelineStage --> IngestionState : run() reads/writes
    IngestionWorker --> BoundedQueue : pop/push IngestionTask
    IngestionWorker --> ArcanumPipelineRegistry : build()
    IngestionWorker --> DagExecutor : execute()
    IngestionWorker --> IngestionDepsOverrideResolver : resolve_for_collection()
    IngestionWorker --> PipelineDeps
    PipelineDeps --> RetryPolicy
    PipelineDeps --> CircuitBreaker
```

`dag.rs` defines `PipelineStage` (`id: StageId`, `deps: Vec<StageId>`,
`run: StageFn`) and `PipelineDAG` (a `Vec<PipelineStage>` built via
`add_stage`), plus three `StageContext` flag constants — `CTX_FORCE`,
`CTX_SKIP`, `CTX_REPLACE` — stages use to signal dedup/force decisions to
each other through the shared `StageContext`. `executor.rs`'s
`DagExecutor::execute` runs stages in dependency-satisfying waves: each
iteration computes the `ready` set (every stage whose `deps` are all in
`completed`), then runs `(stage.run)(ctx.clone())` for every id in
`ready` concurrently via `futures::future::join_all`, each stage getting
its own cloned `ctx`; once every future in the wave has resolved, results
are merged back into the shared `ctx` in deterministic wave order — a
wave-mate's returned ctx is its full cloned snapshot, so an unmodified
copy of key K overwrites an earlier wave-mate's write to K, meaning
wave-sharing stages must not write overlapping `ctx` keys (Implementation
Notes). A wave with no ready stages while stages remain is a cycle
(`ArcanumError::Pipeline { stage: "executor", .. }`); each stage runs
inside an `info_span!("pipeline.stage", stage_id)` and records
`arcanum_pipeline_stages_total`/`arcanum_pipeline_stage_duration_seconds`.

`registry.rs`'s `ArcanumPipelineRegistry` is a `HashMap<String,
TemplateBuilder>`; its `Default` registers five templates by name —
`standard`, `contextual`, `graph`, `raptor`, `full`
(`templates/{standard,contextual,graph,raptor,full}.rs`) — and `build`
returns `ArcanumError::Pipeline { stage: "registry", .. }` for an unknown
name. `contextual`, `graph`, and `raptor` each fall back to
`templates::standard::builder()` when their one required dependency
(`context_enricher`; `entity_extractor` + `graph_store`; `tree_store`) is
`None`; `full` instead builds every stage it can reach unconditionally
and adds `entity_extract` / `tree_embed` + `raptor_build` only when their
deps are `Some`, with no fallback. `stages.rs` holds the `make_*` factory
functions — `make_load_stage` through `make_raptor_build_stage`,
covering load, dedup, cleanup, preprocess, snapshot, the three chunk
stages, context enrich, entity extract, embed (vector and tree), vector
write, and version registration — each closing over a shared
`Arc<Mutex<IngestionState>>` and its own typed dependencies to produce
one `PipelineStage`. `make_vector_write_stage` and `make_raptor_build_stage`
each also take one optional dependency from `PipelineDeps` —
`bm25_index: Option<Arc<Bm25Index>>` and `enricher: Option<Arc<dyn
TextEnricher>>` (reusing `deps.context_enricher`) — and both are
`None`-safe: `vector_write` skips its BM25 batch-write and
`raptor_build` falls back to a placeholder cluster summary when
unconfigured. `stage_failure.rs` defines `StageFailure` (`Core {
stage, error }` / `NonCore { stage, error }`) and `is_core_stage` (`true`
for `load`, `preprocess`, `vector_chunk`, `graph_chunk`, `tree_chunk`,
`embed`, `vector_write`) — see Implementation Notes for how far this
classification actually reaches into the worker's retry decision.

`worker.rs`'s `IngestionWorker` wraps an `ArcanumPipelineRegistry`, a base
`PipelineDeps`, a `ProgressEmitter`, a `BoundedQueue<IngestionTask>`, and
an optional `IngestionDepsOverrideResolver`. `process_next` pops a task
and calls `resolve_task_deps` — with a resolver attached, this rebuilds a
fresh `PipelineDeps` from the resolver's per-collection
`(PerBackendChunkers, Option<ShadowContext>, Option<Arc<dyn
Preprocessor>>)`, cheap-cloning every other field from the base `deps`;
without a resolver, or on a resolution error, it uses the base `deps`
unchanged — then calls the free function `run_task`.

`arcanum-middleware`'s three types back the reliability behavior above.
`queue.rs`'s `BoundedQueue<T>` wraps a bounded `tokio::mpsc` channel:
`push` uses `try_send` (`ArcanumError::QueueFull` instead of blocking)
and `pop` awaits under an internal `tokio::sync::Mutex` on the receiver,
since `mpsc::Receiver::recv` needs `&mut self`. `retry.rs`'s
`RetryPolicy` (`max_attempts`, `base_delay_ms`, `max_delay_ms`)
implements exponential backoff with full jitter: `delay_for_attempt`
computes `cap = min(max_delay_ms, base_delay_ms * 2^attempt)` and returns
a jittered `[0, cap)` delay from an inline LCG, no external `rand`
dependency. `circuit_breaker.rs`'s `CircuitBreaker`
(`Closed`/`Open`/`HalfOpen`, an `AtomicU8`) trips to `Open` once
`failures` crosses `failure_threshold` and self-transitions to `HalfOpen`
once `reset_timeout` elapses since `opened_at`; `allow_request()` blocks
only `Open`, and `record_success` resets `failures` and closes the
circuit unconditionally.

## Runtime Flows

**1. An `IngestionTask`'s journey from queue to DAG completion**
1. `IngestionWorker::process_next` pops a task from `BoundedQueue`, then
   `resolve_task_deps` calls `IngestionDepsOverrideResolver::resolve_for_collection`
   when a resolver is attached — in practice `EngineIngestionDepsResolver`
   (see [Ingestion](ingestion.md) Flow 2 for what it resolves).
2. `run_task` builds a `Source` (from `task.content` if the task carries
   inline bytes, else `Source::from_uri`), constructs a fresh
   `IngestionState`, and calls `registry.build(&task.pipeline_template, ..)`
   to assemble the `PipelineDAG` for the selected template.
3. `DagExecutor::execute` runs stage waves in dependency order: `load` →
   `dedup` → `cleanup` → `preprocess`, then `snapshot`, `vector_chunk`,
   `graph_chunk`, and `tree_chunk` all depend only on `preprocess`, so
   they land in one wave together and run concurrently via
   `futures::future::join_all` (see Key Decisions/Implementation Notes
   for the wave-merge contract this concurrency relies on). `vector_write`
   depends on both `embed` and `snapshot`, so `snapshot_document_id`/
   `snapshot_version_num` are populated before it builds
   `ChunkMetadataRecord`s; `register_version` runs last.
4. On success, `run_task` emits `"ingestion:progress"` — `status:
   "skipped"` if `CTX_SKIP` is set on the final `StageContext`, else
   `status: "completed"` with an `IngestionReport` (`total_chunks`,
   `total_vectors`, `document_fingerprint` from `doc.content_hash()`) whose
   `status` field is `IngestionStatus::PartialSuccess { failed_stages }`
   when `CTX_STAGE_FAILURES` is non-empty (a non-core stage failed and its
   dependents were skipped) and `IngestionStatus::Success` otherwise. A
   skipped dependent that would itself be a core stage is never folded
   into `PartialSuccess` this way: `DagExecutor::execute` aborts with an
   `Err` naming the root non-core failure instead, so that case takes the
   failure/retry path in Flow 2 rather than reaching this step.

**2. Failure, retry, and circuit breaker interaction**
1. Any stage's `run` returning `Err` short-circuits `DagExecutor::execute`
   — logged and returned; earlier waves' side effects (a `cleanup`
   delete, a `snapshot` write) are not rolled back.
2. `run_task`'s error branch increments `arcanum_ingest_docs_total`, then
   checks `deps.retry_policy.should_retry(task_attempt)` — a pure
   `attempt < max_attempts` comparison that does not consult
   `is_core_stage` or `StageFailure` (Implementation Notes), so a
   `graph_chunk` or `register_version` failure retries exactly like a
   `load` failure.
3. If retryable, `run_task` sleeps `RetryPolicy::delay_for_attempt(task_attempt)`
   and pushes a new `IngestionTask` with `attempt: task_attempt + 1` back
   onto the same `BoundedQueue`.
4. Independently, `make_embed_stage`, `make_tree_embed_stage`, and
   `make_vector_write_stage` each call `CircuitBreaker::allow_request()`
   before calling the embedder or vector store, failing fast when the
   breaker is `Open`; a call that goes through records success/failure on
   its breaker, opening it independently of the DAG-level retry loop.

**3. Dedup and cleanup on `DocumentVersionStore`** (see
[Ingestion](ingestion.md) Flow 1 for the store-side contract this relies
on; this account stays at the stage-wiring level)
1. `make_dedup_stage` calls `DocumentVersionStore::get_latest(source_uri,
   collection_id)`: no prior version proceeds as new, a matching
   `content_hash` sets `CTX_SKIP`, a differing hash sets `CTX_REPLACE` —
   unless `CTX_FORCE` (set from `IngestionTask.force`) is already present,
   which sets `CTX_REPLACE` unconditionally without checking the store.
2. `make_cleanup_stage` runs only when `CTX_REPLACE` is set: it calls
   `delete_by_source_uri` on `vector_store` and, if configured,
   `graph_store`/`tree_store`, before `preprocess` runs — it does not
   itself call `supersede_active` (see Implementation Notes).
3. The version actually gets superseded, when it does, from a different
   path: `make_snapshot_stage` calls `get_versioning_policy` and, under
   `VersioningPolicy::Replace` with a prior version present, calls
   `supersede_active(&doc_id)` itself using the `document_id` it just
   read from `get_latest`.

## Key Decisions

Newest first.

### `vector_write`/`raptor_build` wire previously-unused `Bm25Index`/`TextEnricher` dependencies
- **Decision** — `make_vector_write_stage` gained a 5th parameter,
  `bm25_index: Option<Arc<Bm25Index>>` (best-effort batch-write to
  `Bm25Index::index_chunks` after a successful vector-store upsert), and
  `make_raptor_build_stage` gained a 4th parameter, `enricher:
  Option<Arc<dyn TextEnricher>>` (passed to `RaptorBuilder::with_enricher`,
  reusing `deps.context_enricher` rather than a new `PipelineDeps` field).
- **Context** — the commit messages record both as previously-dead
  capability: `Bm25Retriever` "read from an index that ingestion never
  populated"; RAPTOR summaries were "the literal placeholder string ...
  never LLM-generated."
- **Alternatives rejected** — No PR or design doc records an alternative;
  both commits wire an existing unused field/instance into the write path
  rather than introducing a new one.
- **Consequences** — BM25 search and RAPTOR cluster summaries now reflect
  ingested content when configured; both fail open (warn-and-continue for
  BM25, placeholder fallback for RAPTOR on a missing/failing enricher).
  Known follow-up gap, see Implementation Notes: `Bm25Index` has no
  delete-by-`source_uri`, so `make_cleanup_stage`'s replace-path deletes
  don't reach it.
- **Ref** — 2026-07-15, PR #50, commit `b7e81d70`.

### `register_version` deferred until after `vector_write` succeeds
- **Decision** — `make_snapshot_stage` builds a `pending_version:
  DocumentVersion` on `IngestionState` without calling
  `DocumentVersionStore::add_version`; only `make_register_version_stage`
  (`deps: ["vector_write"]`) calls `add_version`, taking the pending
  version out of state.
- **Context** — the PR body lists as a bug fix: "`supersede_active +
  add_version` not in transaction" → "Moved `add_version` to after
  `vector_write` via `pending_version` state field," so "partial failures
  don't leave orphaned version records."
- **Alternatives rejected** — the PR body records the prior in-transaction
  pairing as the bug being fixed, not a considered alternative.
- **Consequences** — a version becomes visible in `DocumentVersionStore`
  only once every store write in the DAG has succeeded; a task whose
  `vector_write` fails leaves no registered version, and
  `register_version` is itself skipped when `CTX_SKIP` is set.
- **Ref** — 2026-06-16, PR #44.

### Shadow-write integration for chunking experiments
- **Decision** — `make_vector_chunk_stage` takes an optional
  `ShadowWriteContext` (challenger `chunker`, `shadow_collection_id`,
  `embedder`, `vector_store`, `vector_store_cb`); when present it
  `tokio::spawn`s a detached task that chunks, embeds, and writes to the
  shadow collection behind its own `CircuitBreaker::allow_request()`
  check, logging every failure via `tracing::warn!` rather than
  propagating it.
- **Context** — PR #41 wired shadow chunking into the pipeline behind an
  `ExperimentService` lifecycle; its own review found "shadow spawn had
  no vector store write." PR #42's fix table records the correction: a
  full `ShadowWriteContext` with "actual vector store upsert in detached
  task," and, per the same fix table, "shadow namespace was raw
  experiment UUID" — replaced with the deterministic
  `"{collection}__shadow_{experiment}"`.
- **Alternatives rejected** — no PR records an alternative to a
  detached, best-effort shadow write; PR #42 treats the original
  write-less path as the bug, not a rejected design.
- **Consequences** — a slow or failing shadow embed/write never delays or
  fails primary ingestion, but its failures are only observable via logs
  and `vector_store_cb`'s own metrics, not the primary `IngestionReport`.
- **Ref** — 2026-06-08, PR #42, building on 2026-06-07, PR #41.

### Chunk stage split into three independent per-backend stages
- **Decision** — the single `make_chunk_stage` was replaced by
  `make_vector_chunk_stage`, `make_graph_chunk_stage`, and
  `make_tree_chunk_stage`, each depending only on `preprocess` and
  writing to its own `IngestionState` field (`chunks`, `graph_chunks`,
  `tree_chunks`).
- **Context** — the PR body frames this as "the structural foundation for
  per-backend chunking," with downstream stages reading the matching
  field (`entity_extract` from `graph_chunks`, `raptor_build` from
  `tree_chunks`, each with a documented backward-compat fallback to the
  vector chunks) and `is_core_stage` updated for all three new stage IDs.
- **Alternatives rejected** — the PR body records this as a direct
  structural replacement, not a choice among live alternatives.
- **Consequences** — the three chunk stages depend on nothing but
  `preprocess`, the structural precondition for concurrent execution;
  `DagExecutor::execute` now runs every stage in a ready wave
  concurrently (Implementation Notes), so this split delivers both
  per-backend isolation and a wall-clock speedup for the
  `vector_chunk`/`graph_chunk`/`tree_chunk` wave.
- **Ref** — 2026-06-07, PR #38.

### Circuit breaker checks wired into `make_embed_stage`/`make_vector_write_stage`
- **Decision** — both stages call `CircuitBreaker::allow_request()` before
  calling the embedder/vector store and `record_success`/`record_failure`
  on the outcome, using the `embedding_cb`/`vector_store_cb` already on
  `PipelineDeps`.
- **Context** — the commit message states the templates "already pass
  `embedding_cb` and `vector_store_cb` as args; this commit wires them
  into the stage execution so the CB guards are actually enforced,"
  noting the wiring "were applied to the working tree during the P1-T3
  refactor but were not staged before the branch was pushed."
- **Alternatives rejected** — No PR or design doc records an alternative;
  the commit presents this as completing work already in flight.
- **Consequences** — before this commit, `PipelineDeps` carried breakers
  every template constructed and passed down but no stage consulted;
  embed/vector-write failures could not trip one.
- **Ref** — 2026-06-01, commit `bee31448`.

## Implementation Notes

- **Retry re-queue does not use `is_core_stage`/`StageFailure` (drift).**
  Both are exported from the crate root, but `run_task`'s retry branch
  calls only `deps.retry_policy.should_retry(task_attempt)` — an
  attempt-count check with no reference to which stage failed.
  `StageFailure` is constructed only in
  `arcanum-pipeline/tests/state_test.rs`; the commit that introduced the
  retry re-queue and these two types together was titled "add
  IngestionReport generation and retry re-queue on core stage failure,"
  but the re-queue condition it shipped was always attempt-count only.
- **`make_cleanup_stage`'s dead `supersede_active` guard was removed
  (resolved).** PR #49 (commit `31c83450`) deleted the guard: it read
  `document_id` from `state.snapshot_document_id`, set only by
  `make_snapshot_stage`, which runs strictly after `cleanup` in every
  template's DAG, so the branch was always dead within a single
  `run_task` call. The now-fully-unused `version_store` parameter was
  renamed to `_version_store` instead, avoiding a signature ripple across
  all 5 templates. The version that actually gets superseded on a
  replace still comes from `make_snapshot_stage` (Runtime Flow 3, step
  3), gated on `VersioningPolicy` rather than `CTX_REPLACE`.
- **`Bm25Index` has no delete-by-`source_uri`, so `make_cleanup_stage`
  can't clean it up on replace (debt).** `make_cleanup_stage` deletes
  stale `vector_store`/`graph_store`/`tree_store` data by `source_uri` on
  replace, but `Bm25Index` only supports single-chunk delete-by-id — a
  replaced document's old BM25 entries persist. PR #50 (commit
  `b7e81d70`) recorded this as an intentionally deferred gap when it
  wired `Bm25Index` into `make_vector_write_stage` (see Key Decisions).
- **Chunk stages now run concurrently within their wave (resolved).**
  `vector_chunk`/`graph_chunk`/`tree_chunk` share no dependency on each
  other; `DagExecutor::execute` runs every stage in a ready wave
  concurrently via `futures::future::join_all`, each on its own cloned
  `ctx`, merging results back into the shared `ctx` in deterministic wave
  order only once the whole wave has resolved (`executor.rs`). The three
  chunk stages write disjoint `IngestionState` fields (`chunks`,
  `graph_chunks`, `tree_chunks`) behind the shared
  `Arc<Mutex<IngestionState>>` and touch no overlapping `ctx` keys, so
  they satisfy the wave-merge contract concurrent wave-mates must follow
  (see the chunk-split Key Decision).
- **Cache invalidation now fires on any genuine content change, not just
  `force` (resolved).** PR #49 (commit `31c83450`) moved the
  `CacheInvalidationBroadcaster::invalidate_document` call from before
  the pipeline ran (gated on `IngestionTask.force`) to after
  `DagExecutor::execute` completes, gated on `!skipped` instead — one
  condition covering force, a genuine content change (dedup's `CTX_REPLACE`),
  and a brand-new document (a harmless no-op). The introducing commit
  (`c7b77c2d`) originally gated the same call on `force || already_seen`
  via a now-deleted `hash_tracker` field (superseded by
  `DocumentVersionStore`, see [Ingestion](ingestion.md)); `already_seen`
  was never replaced until this fix.
- **`PipelineTemplate` enum was removed (resolved).** PR #49 (commit
  `31c83450`) deleted the dead `PipelineTemplate { Standard, Contextual,
  Graph, Raptor, Full, Custom(PipelineDAG) }` enum from `lib.rs` — zero
  references workspace-wide; template selection goes through
  `ArcanumPipelineRegistry::build`'s string name instead, as it already
  did before removal.

## Source Anchors

- `arcanum-pipeline/src/dag.rs`
- `arcanum-pipeline/src/executor.rs`
- `arcanum-pipeline/src/registry.rs`
- `arcanum-pipeline/src/stages.rs`
- `arcanum-pipeline/src/stage_failure.rs`
- `arcanum-pipeline/src/worker.rs`
- `arcanum-pipeline/src/deps.rs`
- `arcanum-pipeline/src/ingestion_state.rs`
- `arcanum-pipeline/src/templates/` (module)
- `arcanum-middleware/src/queue.rs`
- `arcanum-middleware/src/retry.rs`
- `arcanum-middleware/src/circuit_breaker.rs`

<!-- The drift contract: a PR changing files under these anchors updates this page
     or says why not in the PR body. -->

## Related Pages

- [Core](core.md)
- [Ingestion](ingestion.md)
- [Storage](storage.md)
- [Engine](engine.md)
- [Evidence](evidence.md)
- [Evaluation](evaluation.md)
- [Retrieval](retrieval.md)
