use crate::{dag::{PipelineStage, StageContext, CTX_FORCE, CTX_SKIP, CTX_REPLACE}, IngestionState};
use arcanum_core::{traits::*, types::*, ArcanumError};
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
    state: Arc<Mutex<IngestionState>>,
    registry: Arc<dyn DocumentRegistry>,
) -> PipelineStage {
    PipelineStage {
        id: "dedup",
        deps: vec!["load"],
        run: Arc::new(move |mut ctx| {
            let state = state.clone();
            let registry = registry.clone();
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
                    (doc.source_uri.clone(), g.collection_id.clone(), doc.content_hash())
                };
                let entry = registry.get_entry(&source_uri, &collection_id.0).await?;
                match entry {
                    None => {
                        // New document — proceed normally
                    }
                    Some(e) if e.status == RegistryStatus::Replacing => {
                        // Previous cleanup interrupted; resume
                        ctx.insert(CTX_REPLACE.to_string(), serde_json::json!(true));
                    }
                    Some(e) if e.content_hash.as_deref() == Some(content_hash.as_str()) => {
                        // Identical content — skip
                        ctx.insert(CTX_SKIP.to_string(), serde_json::json!(true));
                    }
                    Some(_) => {
                        // Changed content — replace
                        ctx.insert(CTX_REPLACE.to_string(), serde_json::json!(true));
                    }
                }
                Ok(ctx)
            })
        }),
    }
}

pub fn make_cleanup_stage(
    state: Arc<Mutex<IngestionState>>,
    registry: Arc<dyn DocumentRegistry>,
    vector_store: Arc<dyn VectorStore>,
    graph_store: Option<Arc<dyn GraphStore>>,
    tree_store: Option<Arc<dyn TreeStore>>,
) -> PipelineStage {
    PipelineStage {
        id: "cleanup",
        deps: vec!["dedup"],
        run: Arc::new(move |mut ctx| {
            let state = state.clone();
            let registry = registry.clone();
            let vs = vector_store.clone();
            let gs = graph_store.clone();
            let ts = tree_store.clone();
            Box::pin(async move {
                tracing::debug!(stage = "cleanup", "executing cleanup stage");
                let replace = ctx.get(CTX_REPLACE).and_then(|v| v.as_bool()).unwrap_or(false);
                if !replace {
                    return Ok(ctx);
                }
                let (source_uri, collection_id) = {
                    let g = state.lock().await;
                    let doc = g.doc.as_ref().ok_or_else(|| ArcanumError::Pipeline {
                        stage: "cleanup".into(),
                        message: "no doc".into(),
                    })?;
                    (doc.source_uri.clone(), g.collection_id.clone())
                };
                if source_uri.is_empty() {
                    return Err(ArcanumError::Pipeline {
                        stage: "cleanup".into(),
                        message: "document source_uri is empty — cannot safely delete stale store data".into(),
                    });
                }
                let claimed = registry.try_set_replacing(&source_uri, &collection_id.0).await?;
                if !claimed {
                    tracing::debug!(stage = "cleanup", source_uri = %source_uri,
                        "another worker is replacing this document — skipping cleanup");
                    ctx.insert(CTX_SKIP.to_string(), serde_json::json!(true));
                    return Ok(ctx);
                }
                vs.delete_by_source_uri(&collection_id.0, &source_uri).await?;
                if let Some(gs) = &gs {
                    gs.delete_by_source_uri(&collection_id.0, &source_uri).await?;
                }
                if let Some(ts) = &ts {
                    ts.delete_by_source_uri(&collection_id.0, &source_uri).await?;
                }
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
                for c in &mut chunks {
                    c.collection_id = collection_id.clone();
                    c.metadata.0.insert(
                        "source_uri".to_string(),
                        serde_json::Value::String(source_uri.clone()),
                    );
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
                                    c.metadata.0.insert(
                                        "source_uri".to_string(),
                                        serde_json::Value::String(shadow_doc.source_uri.clone()),
                                    );
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
                for c in &mut chunks {
                    c.collection_id = collection_id.clone();
                    c.metadata.0.insert(
                        "source_uri".to_string(),
                        serde_json::Value::String(source_uri.clone()),
                    );
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
                for c in &mut chunks {
                    c.collection_id = collection_id.clone();
                    c.metadata.0.insert(
                        "source_uri".to_string(),
                        serde_json::Value::String(source_uri.clone()),
                    );
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

pub fn make_vector_write_stage(
    state: Arc<Mutex<IngestionState>>,
    vector_store: Arc<dyn VectorStore>,
    vector_store_cb: Arc<arcanum_middleware::CircuitBreaker>,
) -> PipelineStage {
    PipelineStage {
        id: "vector_write",
        deps: vec!["embed"],
        run: Arc::new(move |mut ctx| {
            let state = state.clone();
            let vs = vector_store.clone();
            let cb = vector_store_cb.clone();
            Box::pin(async move {
                tracing::debug!(stage = "vector_write", "executing vector_write stage");
                if skip(&ctx) { return Ok(ctx); }
                if !cb.allow_request() {
                    return Err(arcanum_core::ArcanumError::Storage(
                        "circuit open: vector store unavailable".into()
                    ));
                }
                let (chunks, vectors, collection_id) = {
                    let g = state.lock().await;
                    (g.chunks.clone(), g.vectors.clone(), g.collection_id.clone())
                };
                let indexed: Vec<IndexedChunk> = chunks
                    .into_iter()
                    .zip(vectors)
                    .map(|(chunk, vector)| IndexedChunk {
                        chunk,
                        vector,
                        token_vectors: None,
                        store_id: String::new(),
                    })
                    .collect();
                match vs.upsert(&collection_id.0, indexed).await {
                    Ok(()) => {
                        cb.record_success();
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

#[cfg(test)]
mod test_chunk_source_uri {
    use super::*;
    use arcanum_ingestion::FixedSizeChunker;
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
            doc: Some(doc),
            chunks: vec![],
            graph_chunks: vec![],
            tree_chunks: vec![],
            vectors: vec![],
        }));
        let chunker = Arc::new(FixedSizeChunker::new(512, 0));
        let stage = make_vector_chunk_stage(state.clone(), chunker, None);
        (stage.run)(Default::default()).await.unwrap();
        let chunks = state.lock().await.chunks.clone();
        assert!(!chunks.is_empty(), "expected at least one chunk");
        for chunk in &chunks {
            let uri = chunk.metadata.0.get("source_uri")
                .and_then(|v| v.as_str());
            assert_eq!(uri, Some("samples/api-authentication.md"),
                "chunk metadata must contain source_uri");
        }
    }
}

pub fn make_raptor_build_stage(
    state: Arc<Mutex<IngestionState>>,
    tree_store: Arc<dyn TreeStore>,
    max_depth: u32,
) -> PipelineStage {
    PipelineStage {
        id: "raptor_build",
        deps: vec!["embed"],
        run: Arc::new(move |ctx| {
            let state = state.clone();
            let tree_store = tree_store.clone();
            Box::pin(async move {
                tracing::debug!(stage = "raptor_build", "executing raptor_build stage");
                if skip(&ctx) { return Ok(ctx); }
                let (leaves, collection_id, source_uri) = {
                    let g = state.lock().await;
                    let chunks = if g.tree_chunks.is_empty() { g.chunks.clone() } else { g.tree_chunks.clone() };
                    let leaves: Vec<(String, Vector)> = chunks
                        .iter()
                        .map(|c| c.text.clone())
                        .zip(g.vectors.iter().cloned())
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
