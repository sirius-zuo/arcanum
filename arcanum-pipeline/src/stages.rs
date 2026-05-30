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
                for c in &mut chunks {
                    c.collection_id = collection_id.clone();
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
) -> PipelineStage {
    PipelineStage {
        id: "embed",
        deps: vec!["chunk"],
        run: Arc::new(move |ctx| {
            let state = state.clone();
            let embedder = embedder.clone();
            Box::pin(async move {
                if skip(&ctx) { return Ok(ctx); }
                let texts: Vec<String> =
                    state.lock().await.chunks.iter().map(|c| c.text.clone()).collect();
                let vectors = embedder.embed(texts).await?;
                state.lock().await.vectors = vectors;
                Ok(ctx)
            })
        }),
    }
}

pub fn make_embed_stage_after(
    dep: &'static str,
    state: Arc<Mutex<IngestionState>>,
    embedder: Arc<dyn Embedder>,
) -> PipelineStage {
    let mut stage = make_embed_stage(state, embedder);
    stage.deps = vec![dep];
    stage
}

pub fn make_vector_write_stage(
    state: Arc<Mutex<IngestionState>>,
    vector_store: Arc<dyn VectorStore>,
) -> PipelineStage {
    PipelineStage {
        id: "vector_write",
        deps: vec!["embed"],
        run: Arc::new(move |mut ctx| {
            let state = state.clone();
            let vs = vector_store.clone();
            Box::pin(async move {
                if skip(&ctx) { return Ok(ctx); }
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
                vs.upsert(&collection_id.0, indexed).await?;
                ctx.insert("vector_write_ok".to_string(), serde_json::json!(true));
                Ok(ctx)
            })
        }),
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
                if skip(&ctx) { return Ok(ctx); }
                let (leaves, collection_id) = {
                    let g = state.lock().await;
                    let leaves: Vec<(String, Vector)> = g
                        .chunks
                        .iter()
                        .map(|c| c.text.clone())
                        .zip(g.vectors.iter().cloned())
                        .collect();
                    (leaves, g.collection_id.clone())
                };
                let builder = RaptorBuilder::new(tree_store, max_depth);
                builder.build(&collection_id.0, leaves).await?;
                Ok(ctx)
            })
        }),
    }
}
