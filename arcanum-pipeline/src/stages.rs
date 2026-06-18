use crate::{dag::{PipelineStage, StageContext, CTX_FORCE, CTX_SKIP, CTX_REPLACE}, IngestionState};
use arcanum_core::{traits::{DocumentVersionStore, SnapshotStore, ChunkMetadataStore, *}, types::*,
    types::{ChunkMetadataRecord, DocumentId, DocumentVersion, VersionStatus, VersioningPolicy},
    ArcanumError};
use arcanum_ingestion::{
    LoaderRegistry, PreprocessorRegistry, MimeDetector,
    ContextEnricher, EntityExtractor,
};
use arcanum_middleware::CircuitBreaker;
use arcanum_tree::RaptorBuilder;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Context passed to `make_vector_chunk_stage` for best-effort shadow writes.
/// All writes go to `shadow_collection_id`; failure does not fail primary ingestion.
pub struct ShadowWriteContext {
    pub chunker:              Arc<dyn Chunker>,
    pub shadow_collection_id: String,
    pub embedder:             Arc<dyn Embedder>,
    pub vector_store:         Arc<dyn VectorStore>,
    pub vector_store_cb:      Arc<CircuitBreaker>,
}

impl Clone for ShadowWriteContext {
    fn clone(&self) -> Self {
        Self {
            chunker:              self.chunker.clone(),
            shadow_collection_id: self.shadow_collection_id.clone(),
            embedder:             self.embedder.clone(),
            vector_store:         self.vector_store.clone(),
            vector_store_cb:      self.vector_store_cb.clone(),
        }
    }
}

fn skip(ctx: &StageContext) -> bool {
    ctx.get(CTX_SKIP).and_then(|v| v.as_bool()).unwrap_or(false)
}

pub fn make_load_stage(
    state: Arc<Mutex<IngestionState>>,
    loaders: Arc<LoaderRegistry>,
) -> PipelineStage {
    PipelineStage {
        id: "load",
        deps: vec![],
        run: Arc::new(move |ctx| {
            let state = state.clone();
            let loaders = loaders.clone();
            Box::pin(async move {
                tracing::debug!(stage = "load", "executing load stage");
                let source = state.lock().await.source.clone();
                let mut doc = loaders.load(&source).await?;
                doc.mime_type = MimeDetector::detect(&doc.content, Some(&doc.mime_type));
                state.lock().await.doc = Some(doc);
                Ok(ctx)
            })
        }),
    }
}

pub fn make_dedup_stage(
    state:         Arc<Mutex<IngestionState>>,
    version_store: Arc<dyn DocumentVersionStore>,
) -> PipelineStage {
    PipelineStage {
        id: "dedup",
        deps: vec!["load"],
        run: Arc::new(move |mut ctx| {
            let state         = state.clone();
            let version_store = version_store.clone();
            Box::pin(async move {
                tracing::debug!(stage = "dedup", "executing dedup stage");
                let force = ctx.get(CTX_FORCE).and_then(|v| v.as_bool()).unwrap_or(false);
                if force {
                    ctx.insert(CTX_REPLACE.to_string(), serde_json::json!(true));
                    return Ok(ctx);
                }
                let (source_uri, collection_id, content_hash) = {
                    let g = state.lock().await;
                    let doc = g.doc.as_ref().ok_or_else(|| ArcanumError::Pipeline {
                        stage: "dedup".into(),
                        message: "no doc after load".into(),
                    })?;
                    (doc.source_uri.clone(), g.collection_id.0.clone(), doc.content_hash())
                };
                let latest = version_store.get_latest(&source_uri, &collection_id).await?;
                match latest {
                    None => { /* new document — proceed */ }
                    Some(v) if v.content_hash == content_hash => {
                        ctx.insert(CTX_SKIP.to_string(), serde_json::json!(true));
                    }
                    Some(_) => {
                        ctx.insert(CTX_REPLACE.to_string(), serde_json::json!(true));
                    }
                }
                Ok(ctx)
            })
        }),
    }
}

pub fn make_cleanup_stage(
    state:         Arc<Mutex<IngestionState>>,
    version_store: Arc<dyn DocumentVersionStore>,
    vector_store:  Arc<dyn VectorStore>,
    graph_store:   Option<Arc<dyn GraphStore>>,
    tree_store:    Option<Arc<dyn TreeStore>>,
) -> PipelineStage {
    PipelineStage {
        id: "cleanup",
        deps: vec!["dedup"],
        run: Arc::new(move |ctx| {
            let state         = state.clone();
            let version_store = version_store.clone();
            let vs            = vector_store.clone();
            let gs            = graph_store.clone();
            let ts            = tree_store.clone();
            Box::pin(async move {
                tracing::debug!(stage = "cleanup", "executing cleanup stage");
                let replace = ctx.get(CTX_REPLACE).and_then(|v| v.as_bool()).unwrap_or(false);
                if !replace {
                    return Ok(ctx);
                }
                let (source_uri, collection_id, doc_id) = {
                    let g = state.lock().await;
                    let doc = g.doc.as_ref().ok_or_else(|| ArcanumError::Pipeline {
                        stage: "cleanup".into(),
                        message: "no doc".into(),
                    })?;
                    (doc.source_uri.clone(), g.collection_id.0.clone(), g.snapshot_document_id.clone())
                };
                if source_uri.is_empty() {
                    return Err(ArcanumError::Pipeline {
                        stage: "cleanup".into(),
                        message: "document source_uri is empty — cannot safely delete stale store data".into(),
                    });
                }
                if let Some(document_id) = &doc_id {
                    version_store.supersede_active(document_id).await?;
                }
                vs.delete_by_source_uri(&collection_id, &source_uri).await?;
                if let Some(gs) = &gs {
                    gs.delete_by_source_uri(&collection_id, &source_uri).await?;
                }
                if let Some(ts) = &ts {
                    ts.delete_by_source_uri(&collection_id, &source_uri).await?;
                }
                Ok(ctx)
            })
        }),
    }
}

pub fn make_snapshot_stage(
    state:         Arc<Mutex<IngestionState>>,
    version_store: Arc<dyn DocumentVersionStore>,
    snapshot_store: Arc<dyn SnapshotStore>,
) -> PipelineStage {
    PipelineStage {
        id: "snapshot",
        deps: vec!["preprocess"],
        run: Arc::new(move |ctx| {
            let state          = state.clone();
            let version_store  = version_store.clone();
            let snapshot_store = snapshot_store.clone();
            Box::pin(async move {
                tracing::debug!(stage = "snapshot", "executing snapshot stage");
                if skip(&ctx) { return Ok(ctx); }

                let (source_uri, collection_id, content_hash, mime_type, raw_content, canonical_json) = {
                    let g = state.lock().await;
                    let doc = g.doc.as_ref().ok_or_else(|| ArcanumError::Pipeline {
                        stage: "snapshot".into(),
                        message: "no doc".into(),
                    })?;
                    (
                        doc.source_uri.clone(),
                        g.collection_id.0.clone(),
                        doc.content_hash(),
                        doc.mime_type.clone(),
                        g.raw_content.clone().unwrap_or_else(|| doc.content.clone()),
                        g.canonical_json.clone(),
                    )
                };

                // Determine stable document_id and next version_num.
                let latest = version_store.get_latest(&source_uri, &collection_id).await?;
                let doc_id = match &latest {
                    Some(v) => v.document_id.clone(),
                    None    => DocumentId::new(),
                };
                let version_num = latest.as_ref().map(|v| v.version_num + 1).unwrap_or(1);

                // Apply versioning policy.
                let policy = version_store.get_versioning_policy(&collection_id).await?;
                if matches!(policy, VersioningPolicy::Replace) {
                    if latest.is_some() {
                        version_store.supersede_active(&doc_id).await?;
                    }
                }

                // Persist raw bytes + canonical sidecar.
                let location = snapshot_store.store(
                    &doc_id,
                    version_num,
                    &raw_content,
                    canonical_json.as_ref(),
                ).await?;

                // Build the version record but do NOT register it yet.
                // make_register_version_stage runs after vector_write and calls add_version().
                // This ensures the version is only visible if all stores are written.
                let pending = DocumentVersion {
                    document_id:   doc_id.clone(),
                    version_num,
                    source_uri:    source_uri.clone(),
                    collection_id: collection_id.clone(),
                    content_hash,
                    snapshot_uri:  location.raw_uri.clone(),
                    canonical_uri: location.canonical_uri.clone(),
                    mime_type,
                    status:        VersionStatus::Active,
                    ingested_at:   chrono::Utc::now(),
                    extra:         std::collections::HashMap::new(),
                };

                // Write results back to state for chunk stage.
                let mut g = state.lock().await;
                g.snapshot_document_id = Some(doc_id);
                g.snapshot_version_num = Some(version_num);
                g.snapshot_uri         = Some(location.raw_uri);
                g.canonical_uri        = location.canonical_uri;
                g.pending_version      = Some(pending);

                Ok(ctx)
            })
        }),
    }
}

pub fn make_preprocess_stage(
    state: Arc<Mutex<IngestionState>>,
    preprocessors: Arc<PreprocessorRegistry>,
) -> PipelineStage {
    PipelineStage {
        id: "preprocess",
        deps: vec!["cleanup"],
        run: Arc::new(move |ctx| {
            let state = state.clone();
            let pp = preprocessors.clone();
            Box::pin(async move {
                tracing::debug!(stage = "preprocess", "executing preprocess stage");
                if skip(&ctx) { return Ok(ctx); }
                let doc = state.lock().await.doc.clone().ok_or_else(|| {
                    ArcanumError::Pipeline { stage: "preprocess".into(), message: "no doc".into() }
                })?;
                let processed = pp.process(doc).await?;
                // Capture canonical JSON if the preprocessor produced one.
                let canonical = pp.canonical(&processed.id);
                if canonical.is_some() {
                    state.lock().await.canonical_json = canonical;
                }
                state.lock().await.doc = Some(processed);
                Ok(ctx)
            })
        }),
    }
}

pub fn make_vector_chunk_stage(
    state: Arc<Mutex<IngestionState>>,
    chunker: Arc<dyn Chunker>,
    shadow: Option<ShadowWriteContext>, // was: Option<(Arc<dyn Chunker>, String)>
) -> PipelineStage {
    PipelineStage {
        id: "vector_chunk",
        deps: vec!["preprocess"],
        run: Arc::new(move |ctx| {
            let state = state.clone();
            let chunker = chunker.clone();
            let shadow = shadow.clone();
            Box::pin(async move {
                tracing::debug!(stage = "vector_chunk", "executing vector chunk stage");
                if skip(&ctx) { return Ok(ctx); }
                let (doc, collection_id) = {
                    let g = state.lock().await;
                    (
                        g.doc.clone().ok_or_else(|| ArcanumError::Pipeline {
                            stage: "vector_chunk".into(),
                            message: "no doc".into(),
                        })?,
                        g.collection_id.clone(),
                    )
                };
                // Primary chunking
                let mut chunks = chunker.chunk(&doc).await?;
                let source_uri = doc.source_uri.clone();
                let (snapshot_version_num, snapshot_uri, canonical_uri) = {
                    let g = state.lock().await;
                    (
                        g.snapshot_version_num,
                        g.snapshot_uri.clone(),
                        g.canonical_uri.clone(),
                    )
                };
                for c in &mut chunks {
                    c.collection_id = collection_id.clone();
                    // Attach ChunkProvenance with document/version/source tracking.
                    c.provenance = arcanum_core::types::ChunkProvenance {
                        document_version: snapshot_version_num.unwrap_or(0),
                        source_uri:       source_uri.clone(),
                        snapshot_uri:     snapshot_uri.clone().unwrap_or_default(),
                        canonical_uri:    canonical_uri.clone(),
                        page:             None,
                        section:          None,
                        block_ids:        vec![],
                    };
                }
                state.lock().await.chunks = chunks;

                // Shadow write — best-effort, detached task, failure does not fail primary.
                // Writes to shadow_collection_id; uses shadow's chunker + embedder + vector_store.
                if let Some(sw) = shadow {
                    let shadow_doc = doc.clone();
                    tokio::spawn(async move {
                        match sw.chunker.chunk(&shadow_doc).await {
                            Ok(mut shadow_chunks) => {
                                for c in &mut shadow_chunks {
                                    c.collection_id = CollectionId(sw.shadow_collection_id.clone());
                                    c.provenance.source_uri = shadow_doc.source_uri.clone();
                                }
                                let texts: Vec<String> =
                                    shadow_chunks.iter().map(|c| c.text.clone()).collect();
                                if !sw.vector_store_cb.allow_request() {
                                    tracing::warn!(
                                        "shadow: vector_store circuit open — skipping shadow write"
                                    );
                                    return;
                                }
                                match sw.embedder.embed(texts).await {
                                    Ok(vectors) => {
                                        sw.vector_store_cb.record_success();
                                        let indexed: Vec<IndexedChunk> = shadow_chunks
                                            .into_iter()
                                            .zip(vectors)
                                            .map(|(chunk, vector)| IndexedChunk {
                                                chunk,
                                                vector,
                                                token_vectors: None,
                                                store_id:      String::new(),
                                            })
                                            .collect();
                                        if let Err(e) = sw.vector_store
                                            .upsert(&sw.shadow_collection_id, indexed)
                                            .await
                                        {
                                            tracing::warn!(err = ?e,
                                                "shadow vector write failed — ignoring");
                                        }
                                    }
                                    Err(e) => {
                                        tracing::warn!(err = ?e,
                                            "shadow embedding failed — ignoring");
                                    }
                                }
                            }
                            Err(e) => {
                                tracing::warn!(err = ?e, "shadow chunking failed — ignoring");
                            }
                        }
                    });
                }

                Ok(ctx)
            })
        }),
    }
}

pub fn make_graph_chunk_stage(
    state: Arc<Mutex<IngestionState>>,
    chunker: Arc<dyn Chunker>,
) -> PipelineStage {
    PipelineStage {
        id: "graph_chunk",
        deps: vec!["preprocess"],
        run: Arc::new(move |ctx| {
            let state = state.clone();
            let chunker = chunker.clone();
            Box::pin(async move {
                tracing::debug!(stage = "graph_chunk", "executing graph chunk stage");
                if skip(&ctx) { return Ok(ctx); }
                let (doc, collection_id) = {
                    let g = state.lock().await;
                    (
                        g.doc.clone().ok_or_else(|| ArcanumError::Pipeline {
                            stage: "graph_chunk".into(),
                            message: "no doc".into(),
                        })?,
                        g.collection_id.clone(),
                    )
                };
                let mut chunks = chunker.chunk(&doc).await?;
                let source_uri = doc.source_uri.clone();
                let (snapshot_version_num, snapshot_uri, canonical_uri) = {
                    let g = state.lock().await;
                    (
                        g.snapshot_version_num,
                        g.snapshot_uri.clone(),
                        g.canonical_uri.clone(),
                    )
                };
                for c in &mut chunks {
                    c.collection_id = collection_id.clone();
                    c.provenance = arcanum_core::types::ChunkProvenance {
                        document_version: snapshot_version_num.unwrap_or(0),
                        source_uri:       source_uri.clone(),
                        snapshot_uri:     snapshot_uri.clone().unwrap_or_default(),
                        canonical_uri:    canonical_uri.clone(),
                        page:             None,
                        section:          None,
                        block_ids:        vec![],
                    };
                }
                state.lock().await.graph_chunks = chunks;
                Ok(ctx)
            })
        }),
    }
}

pub fn make_tree_chunk_stage(
    state: Arc<Mutex<IngestionState>>,
    chunker: Arc<dyn Chunker>,
) -> PipelineStage {
    PipelineStage {
        id: "tree_chunk",
        deps: vec!["preprocess"],
        run: Arc::new(move |ctx| {
            let state = state.clone();
            let chunker = chunker.clone();
            Box::pin(async move {
                tracing::debug!(stage = "tree_chunk", "executing tree chunk stage");
                if skip(&ctx) { return Ok(ctx); }
                let (doc, collection_id) = {
                    let g = state.lock().await;
                    (
                        g.doc.clone().ok_or_else(|| ArcanumError::Pipeline {
                            stage: "tree_chunk".into(),
                            message: "no doc".into(),
                        })?,
                        g.collection_id.clone(),
                    )
                };
                let mut chunks = chunker.chunk(&doc).await?;
                let source_uri = doc.source_uri.clone();
                let (snapshot_version_num, snapshot_uri, canonical_uri) = {
                    let g = state.lock().await;
                    (
                        g.snapshot_version_num,
                        g.snapshot_uri.clone(),
                        g.canonical_uri.clone(),
                    )
                };
                for c in &mut chunks {
                    c.collection_id = collection_id.clone();
                    c.provenance = arcanum_core::types::ChunkProvenance {
                        document_version: snapshot_version_num.unwrap_or(0),
                        source_uri:       source_uri.clone(),
                        snapshot_uri:     snapshot_uri.clone().unwrap_or_default(),
                        canonical_uri:    canonical_uri.clone(),
                        page:             None,
                        section:          None,
                        block_ids:        vec![],
                    };
                }
                state.lock().await.tree_chunks = chunks;
                Ok(ctx)
            })
        }),
    }
}

pub fn make_context_enrich_stage(
    state: Arc<Mutex<IngestionState>>,
    enricher: Arc<dyn TextEnricher>,
) -> PipelineStage {
    PipelineStage {
        id: "context_enrich",
        deps: vec!["vector_chunk"],
        run: Arc::new(move |ctx| {
            let state = state.clone();
            let enricher = enricher.clone();
            Box::pin(async move {
                tracing::debug!(stage = "context_enrich", "executing context_enrich stage");
                if skip(&ctx) { return Ok(ctx); }
                let ce = ContextEnricher::new(enricher);
                let (chunks, doc_context) = {
                    let g = state.lock().await;
                    let title = g
                        .doc
                        .as_ref()
                        .map(|d| d.source_uri.clone())
                        .unwrap_or_default();
                    (g.chunks.clone(), title)
                };
                let mut enriched = Vec::with_capacity(chunks.len());
                for chunk in chunks {
                    enriched.push(ce.enrich_chunk(chunk, &doc_context).await?);
                }
                state.lock().await.chunks = enriched;
                Ok(ctx)
            })
        }),
    }
}

pub fn make_entity_extract_stage(
    state: Arc<Mutex<IngestionState>>,
    enricher: Arc<dyn TextEnricher>,
    graph_store: Arc<dyn GraphStore>,
) -> PipelineStage {
    PipelineStage {
        id: "entity_extract",
        deps: vec!["graph_chunk"],
        run: Arc::new(move |ctx| {
            let state = state.clone();
            let enricher = enricher.clone();
            let gs = graph_store.clone();
            Box::pin(async move {
                tracing::debug!(stage = "entity_extract", "executing entity_extract stage");
                if skip(&ctx) { return Ok(ctx); }
                let extractor = EntityExtractor::new(enricher);
                let (chunks, collection_id) = {
                    let g = state.lock().await;
                    if g.graph_chunks.is_empty() {
                        // No graph chunks produced — skip entity extraction rather than using
                        // vector chunks at the wrong granularity (finding #8).
                        return Ok(ctx);
                    }
                    (g.graph_chunks.clone(), g.collection_id.clone())
                };
                let mut all_entities = Vec::new();
                let mut all_relations = Vec::new();
                for chunk in &chunks {
                    let (mut entities, relations) = extractor.extract(chunk).await?;
                    // Stamp each entity with the collection so the store can scope it.
                    for e in &mut entities {
                        e.collection_id = collection_id.0.clone();
                    }
                    all_entities.extend(entities);
                    all_relations.extend(relations);
                }
                gs.upsert_entities(&collection_id.0, all_entities).await?;
                gs.upsert_relations(&collection_id.0, all_relations).await?;
                Ok(ctx)
            })
        }),
    }
}

pub fn make_embed_stage(
    state: Arc<Mutex<IngestionState>>,
    embedder: Arc<dyn Embedder>,
    embedding_cb: Arc<arcanum_middleware::CircuitBreaker>,
) -> PipelineStage {
    PipelineStage {
        id: "embed",
        deps: vec!["vector_chunk"],
        run: Arc::new(move |ctx| {
            let state = state.clone();
            let embedder = embedder.clone();
            let cb = embedding_cb.clone();
            Box::pin(async move {
                tracing::debug!(stage = "embed", "executing embed stage");
                if skip(&ctx) { return Ok(ctx); }
                if !cb.allow_request() {
                    return Err(arcanum_core::ArcanumError::Embedding(
                        "circuit open: embedding provider unavailable".into()
                    ));
                }
                let texts: Vec<String> =
                    state.lock().await.chunks.iter().map(|c| c.text.clone()).collect();
                match embedder.embed(texts).await {
                    Ok(vectors) => {
                        cb.record_success();
                        state.lock().await.vectors = vectors;
                        Ok(ctx)
                    }
                    Err(e) => {
                        cb.record_failure();
                        Err(e)
                    }
                }
            })
        }),
    }
}

pub fn make_embed_stage_after(
    dep: &'static str,
    state: Arc<Mutex<IngestionState>>,
    embedder: Arc<dyn Embedder>,
    embedding_cb: Arc<arcanum_middleware::CircuitBreaker>,
) -> PipelineStage {
    let mut stage = make_embed_stage(state, embedder, embedding_cb);
    stage.deps = vec![dep];
    stage
}

pub fn make_tree_embed_stage(
    state: Arc<Mutex<IngestionState>>,
    embedder: Arc<dyn Embedder>,
    embedding_cb: Arc<arcanum_middleware::CircuitBreaker>,
) -> PipelineStage {
    PipelineStage {
        id: "tree_embed",
        deps: vec!["tree_chunk"],
        run: Arc::new(move |ctx| {
            let state = state.clone();
            let embedder = embedder.clone();
            let cb = embedding_cb.clone();
            Box::pin(async move {
                tracing::debug!(stage = "tree_embed", "executing tree_embed stage");
                if skip(&ctx) { return Ok(ctx); }
                let texts: Vec<String> = state.lock().await.tree_chunks.iter()
                    .map(|c| c.text.clone()).collect();
                if texts.is_empty() {
                    return Ok(ctx); // no tree chunks — leave tree_vectors empty
                }
                if !cb.allow_request() {
                    return Err(arcanum_core::ArcanumError::Embedding(
                        "circuit open: tree embedding unavailable".into()
                    ));
                }
                match embedder.embed(texts).await {
                    Ok(vectors) => {
                        cb.record_success();
                        state.lock().await.tree_vectors = vectors;
                        Ok(ctx)
                    }
                    Err(e) => {
                        cb.record_failure();
                        Err(e)
                    }
                }
            })
        }),
    }
}

pub fn make_vector_write_stage(
    state: Arc<Mutex<IngestionState>>,
    vector_store: Arc<dyn VectorStore>,
    vector_store_cb: Arc<arcanum_middleware::CircuitBreaker>,
    chunk_metadata_store: Option<Arc<dyn ChunkMetadataStore>>,
) -> PipelineStage {
    PipelineStage {
        id: "vector_write",
        // Depends on "snapshot" (in addition to "embed") so that
        // state.snapshot_document_id/snapshot_version_num are guaranteed to be populated
        // before this stage builds chunk metadata records. Without this, "snapshot" and
        // "vector_chunk" are sibling branches off "preprocess" with no ordering between
        // them, and vector_write could run before the document_id is known.
        deps: vec!["embed", "snapshot"],
        run: Arc::new(move |mut ctx| {
            let state = state.clone();
            let vs = vector_store.clone();
            let cb = vector_store_cb.clone();
            let cms = chunk_metadata_store.clone();
            Box::pin(async move {
                tracing::debug!(stage = "vector_write", "executing vector_write stage");
                if skip(&ctx) { return Ok(ctx); }
                if !cb.allow_request() {
                    return Err(arcanum_core::ArcanumError::Storage(
                        "circuit open: vector store unavailable".into()
                    ));
                }
                let (chunks, vectors, collection_id, doc_id, version_num) = {
                    let g = state.lock().await;
                    (
                        g.chunks.clone(),
                        g.vectors.clone(),
                        g.collection_id.clone(),
                        g.snapshot_document_id.clone(),
                        g.snapshot_version_num,
                    )
                };

                let indexed: Vec<IndexedChunk> = chunks
                    .into_iter()
                    .zip(vectors.into_iter())
                    .map(|(chunk, vector)| IndexedChunk {
                        chunk,
                        vector,
                        token_vectors: None,
                        store_id: String::new(),
                    })
                    .collect();

                // Build chunk metadata records before the vector write (which consumes
                // `indexed`), but only persist them after the vector write succeeds — a
                // failed vector write must not leave orphaned metadata rows behind.
                let metadata_records: Option<Vec<ChunkMetadataRecord>> = if cms.is_some() {
                    match (&doc_id, version_num) {
                        (Some(doc_id), Some(version_num)) => Some(
                            indexed.iter().map(|chunk| ChunkMetadataRecord {
                                chunk_id:      chunk.chunk.id.clone(),
                                document_id:   doc_id.clone(),
                                collection_id: collection_id.0.clone(),
                                version_num,
                                source_uri:    chunk.chunk.provenance.source_uri.clone(),
                                snapshot_uri:  chunk.chunk.provenance.snapshot_uri.clone(),
                                canonical_uri: chunk.chunk.provenance.canonical_uri.clone(),
                                page:          chunk.chunk.provenance.page,
                                section:       chunk.chunk.provenance.section.clone(),
                                block_ids:     chunk.chunk.provenance.block_ids.clone(),
                                offset_start:  chunk.chunk.position.start,
                                offset_end:    chunk.chunk.position.end,
                                ingested_at:   chrono::Utc::now(),
                            }).collect()
                        ),
                        _ => {
                            tracing::warn!(
                                "snapshot_document_id/version_num not set — skipping chunk metadata write"
                            );
                            None
                        }
                    }
                } else {
                    None
                };

                match vs.upsert(&collection_id.0, indexed).await {
                    Ok(()) => {
                        cb.record_success();
                        if let (Some(cms), Some(records)) = (&cms, metadata_records) {
                            for meta in &records {
                                if let Err(e) = cms.put(meta).await {
                                    tracing::warn!(err = ?e, "chunk metadata write failed — continuing");
                                }
                            }
                        }
                        ctx.insert("vector_write_ok".to_string(), serde_json::json!(true));
                        Ok(ctx)
                    }
                    Err(e) => {
                        cb.record_failure();
                        Err(e)
                    }
                }
            })
        }),
    }
}

/// Registers the document version in the version store.
/// Runs AFTER vector_write so the version is only registered when all store writes succeed.
/// If this stage is skipped (dedup saw no change), no registration happens.
pub fn make_register_version_stage(
    state:         Arc<Mutex<IngestionState>>,
    version_store: Arc<dyn DocumentVersionStore>,
) -> PipelineStage {
    PipelineStage {
        id: "register_version",
        deps: vec!["vector_write"],
        run: Arc::new(move |ctx| {
            let state         = state.clone();
            let version_store = version_store.clone();
            Box::pin(async move {
                tracing::debug!(stage = "register_version", "executing register_version stage");
                if skip(&ctx) { return Ok(ctx); }
                let pending = state.lock().await.pending_version.take();
                if let Some(version) = pending {
                    version_store.add_version(version).await?;
                    tracing::debug!(stage = "register_version", "version registered");
                }
                Ok(ctx)
            })
        }),
    }
}

#[cfg(test)]
mod test_chunk_source_uri {
    use super::*;
    use arcanum_ingestion::FixedSizeChunker;
    use arcanum_core::traits::NoOpDocumentVersionStore;
    use std::sync::Arc;
    use tokio::sync::Mutex;

    #[tokio::test]
    async fn chunk_stage_sets_source_uri_in_metadata() {
        let doc = RawDocument {
            id: DocumentId::new(),
            content: b"Hello world. This is a test document.".to_vec(),
            mime_type: "text/plain".to_string(),
            source_uri: "samples/api-authentication.md".to_string(),
            metadata: Default::default(),
        };
        let collection = CollectionId("devforge".to_string());
        let state = Arc::new(Mutex::new(IngestionState {
            source: arcanum_core::traits::Source::Raw {
                content: doc.content.clone(),
                mime_hint: Some("text/plain".to_string()),
                uri: doc.source_uri.clone(),
            },
            collection_id: collection.clone(),
            doc: Some(doc.clone()),
            chunks: vec![],
            graph_chunks: vec![],
            tree_chunks: vec![],
            vectors: vec![],
            tree_vectors: vec![],
            raw_content:   Some(doc.content.clone()),
            canonical_json: None,
            snapshot_document_id: None,
            snapshot_version_num: None,
            snapshot_uri: None,
            canonical_uri: None,
            pending_version: None,
        }));
        let chunker = Arc::new(FixedSizeChunker::new(512, 0));
        let stage = make_vector_chunk_stage(state.clone(), chunker, None);
        (stage.run)(Default::default()).await.unwrap();
        let chunks = state.lock().await.chunks.clone();
        assert!(!chunks.is_empty(), "expected at least one chunk");
        for chunk in &chunks {
            assert_eq!(chunk.provenance.source_uri.as_str(), "samples/api-authentication.md",
                "chunk provenance must contain source_uri");
        }
    }

    #[tokio::test]
    async fn register_version_stage_is_skipped_when_pipeline_is_skipped() {
        let vs = Arc::new(NoOpDocumentVersionStore);
        let state = Arc::new(Mutex::new(IngestionState {
            source: arcanum_core::traits::Source::Raw {
                content: vec![],
                mime_hint: None,
                uri: "test://".into(),
            },
            collection_id: CollectionId("c".into()),
            doc: None,
            chunks: vec![], graph_chunks: vec![], tree_chunks: vec![],
            vectors: vec![], tree_vectors: vec![],
            raw_content: None, canonical_json: None,
            snapshot_document_id: None, snapshot_version_num: None,
            snapshot_uri: None, canonical_uri: None, pending_version: None,
        }));

        // Insert a skip flag.
        let mut ctx = StageContext::new();
        ctx.insert(CTX_SKIP.to_string(), serde_json::json!(true));

        let stage = make_register_version_stage(state.clone(), vs);
        let result_ctx = (stage.run)(ctx).await.unwrap();
        assert!(result_ctx.get(CTX_SKIP).and_then(|v| v.as_bool()).unwrap_or(false));
        // No panic means add_version was not called.
    }
}

pub fn make_raptor_build_stage(
    state: Arc<Mutex<IngestionState>>,
    tree_store: Arc<dyn TreeStore>,
    max_depth: u32,
) -> PipelineStage {
    PipelineStage {
        id: "raptor_build",
        deps: vec!["tree_embed"],
        run: Arc::new(move |ctx| {
            let state = state.clone();
            let tree_store = tree_store.clone();
            Box::pin(async move {
                tracing::debug!(stage = "raptor_build", "executing raptor_build stage");
                if skip(&ctx) { return Ok(ctx); }
                let (leaves, collection_id, source_uri) = {
                    let g = state.lock().await;
                    // Use tree-specific chunks and embeddings when available (per-backend chunkers).
                    // Fall back to primary vector chunks only when tree backend uses the same
                    // chunker and tree_chunks was not separately populated (backward-compatible).
                    let (chunks, vectors) = if !g.tree_chunks.is_empty() && !g.tree_vectors.is_empty() {
                        (g.tree_chunks.clone(), g.tree_vectors.clone())
                    } else {
                        (g.chunks.clone(), g.vectors.clone())
                    };
                    if chunks.len() != vectors.len() {
                        return Err(arcanum_core::ArcanumError::Pipeline {
                            stage: "raptor_build".into(),
                            message: format!(
                                "chunk/vector count mismatch: {} chunks vs {} vectors — \
                                 ensure make_tree_embed_stage runs before make_raptor_build_stage",
                                chunks.len(), vectors.len()
                            ),
                        });
                    }
                    let leaves: Vec<(ChunkId, String, Vector)> = chunks
                        .into_iter()
                        .zip(vectors.into_iter())
                        .map(|(chunk, vec)| (chunk.id, chunk.text, vec))
                        .collect();
                    let source_uri = g.doc.as_ref()
                        .map(|d| d.source_uri.clone())
                        .unwrap_or_else(|| {
                            tracing::warn!(stage = "raptor_build", "doc is None — tree nodes will have empty source_uri and cannot be cleaned up by source");
                            String::new()
                        });
                    (leaves, g.collection_id.clone(), source_uri)
                };
                let builder = RaptorBuilder::new(tree_store, max_depth);
                builder.build(&collection_id.0, &source_uri, leaves).await?;
                Ok(ctx)
            })
        }),
    }
}
