use crate::{dag::{PipelineStage, StageContext}, IngestionState};
use arcanum_core::{traits::*, types::*, ArcanumError};
use arcanum_ingestion::{
    LoaderRegistry, PreprocessorRegistry, DocumentHashTracker, MimeDetector,
    ContextEnricher, EntityExtractor,
};
use arcanum_tree::RaptorBuilder;
use std::sync::Arc;
use tokio::sync::Mutex;

fn skip(ctx: &StageContext) -> bool {
    ctx.get("__skip").and_then(|v| v.as_bool()).unwrap_or(false)
}

pub fn make_load_stage(
    state: Arc<Mutex<IngestionState>>,
    loaders: Arc<LoaderRegistry>,
    hash_tracker: Arc<DocumentHashTracker>,
) -> PipelineStage {
    PipelineStage {
        id: "load",
        deps: vec![],
        run: Arc::new(move |mut ctx| {
            let state = state.clone();
            let loaders = loaders.clone();
            let ht = hash_tracker.clone();
            Box::pin(async move {
                tracing::debug!(stage = "load", "executing load stage");
                let source = state.lock().await.source.clone();
                let mut doc = loaders.load(&source).await?;
                doc.mime_type = MimeDetector::detect(&doc.content, Some(&doc.mime_type));
                if ht.seen_unchanged(&doc.source_uri, &doc.content).await {
                    ctx.insert("__skip".to_string(), serde_json::json!(true));
                    return Ok(ctx);
                }
                state.lock().await.doc = Some(doc);
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
        deps: vec!["load"],
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

pub fn make_chunk_stage(
    state: Arc<Mutex<IngestionState>>,
    chunker: Arc<dyn Chunker>,
) -> PipelineStage {
    PipelineStage {
        id: "chunk",
        deps: vec!["preprocess"],
        run: Arc::new(move |ctx| {
            let state = state.clone();
            let chunker = chunker.clone();
            Box::pin(async move {
                tracing::debug!(stage = "chunk", "executing chunk stage");
                if skip(&ctx) { return Ok(ctx); }
                let (doc, collection_id) = {
                    let g = state.lock().await;
                    (
                        g.doc.clone().ok_or_else(|| ArcanumError::Pipeline {
                            stage: "chunk".into(),
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
                state.lock().await.chunks = chunks;
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
        deps: vec!["chunk"],
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
        deps: vec!["chunk"],
        run: Arc::new(move |ctx| {
            let state = state.clone();
            let enricher = enricher.clone();
            let gs = graph_store.clone();
            Box::pin(async move {
                tracing::debug!(stage = "entity_extract", "executing entity_extract stage");
                if skip(&ctx) { return Ok(ctx); }
                let extractor = EntityExtractor::new(enricher);
                let chunks = state.lock().await.chunks.clone();
                let mut all_entities = Vec::new();
                let mut all_relations = Vec::new();
                for chunk in &chunks {
                    let (entities, relations) = extractor.extract(chunk).await?;
                    all_entities.extend(entities);
                    all_relations.extend(relations);
                }
                gs.upsert_entities(all_entities).await?;
                gs.upsert_relations(all_relations).await?;
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
        deps: vec!["chunk"],
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
            vectors: vec![],
        }));
        let chunker = Arc::new(FixedSizeChunker::new(512, 0));
        let stage = make_chunk_stage(state.clone(), chunker);
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
                    let leaves: Vec<(String, Vector)> = g
                        .chunks
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
