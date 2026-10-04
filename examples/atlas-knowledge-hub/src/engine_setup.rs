use crate::ollama_generator::OllamaGenerator;
use crate::settings::Settings;
use crate::state::{AtlasState, GeneratorMeta};
use anyhow::{Context, Result};
use arcanum_core::config::{ArcanumConfig, OrchestrationMode};
use arcanum_core::traits::{
    Embedder, Generator, GraphStore, InMemoryChunkMetadataStore, Preprocessor, TextEnricher,
    TreeStore, VectorStore,
};
use arcanum_core::types::RawDocument;
use arcanum_core::ArcanumError;
use arcanum_engine::ArcanumEngineBuilder;
use arcanum_evidence::DefaultEvidenceResolver;
use arcanum_graph::InMemoryGraphStore;
use arcanum_ingestion::{
    LocalOperationPayloadStore, LocalSnapshotStore, SqliteDocumentVersionStore,
};
use arcanum_models::{AnthropicGenerator, OllamaProvider};
use arcanum_tree::InMemoryTreeStore;
use arcanum_vector::{Bm25Index, LanceDbStore};
use rand::distributions::Alphanumeric;
use rand::Rng;
use std::path::Path;
use std::sync::Arc;

/// Collection id used by the whole showcase.
pub const COLLECTION: &str = "halcyon";

const CLAUDE_MODEL: &str = "claude-sonnet-5-5";
const GENERATOR_MAX_OUTPUT_TOKENS: u32 = 1024;

/// The sample corpus is already text (markdown), so preprocessing is the identity. The standard
/// pipeline template fails without a `default` preprocessor, and passing the bytes through
/// keeps the raw snapshot identical to the text the chunk offsets index.
struct PassThroughPreprocessor;

#[async_trait::async_trait]
impl Preprocessor for PassThroughPreprocessor {
    async fn process(&self, doc: RawDocument) -> arcanum_core::Result<RawDocument> {
        Ok(doc)
    }
}

/// The model-facing dependencies. Tests inject fakes; production uses Ollama.
pub struct ModelDeps {
    pub embedder: Arc<dyn Embedder>,
    /// Length of the vectors `embedder` returns. `Embedder::dimension()` cannot be trusted
    /// (`OllamaProvider` reports 0), so the startup warm-up uses this value instead.
    pub embed_dimension: usize,
    pub enricher: Option<Arc<dyn TextEnricher>>,
    pub generators: Vec<(String, Arc<dyn Generator>, u32)>,
    pub default_generator: String,
    pub judge: Option<String>,
}

impl ModelDeps {
    pub fn ollama(settings: &Settings) -> ModelDeps {
        let ollama = settings.ollama_url.trim_end_matches('/');
        let embedder = Arc::new(OllamaProvider::new(
            ollama,
            "nomic-embed-text",
            &settings.chat_model,
        ));
        let enricher = Arc::new(OllamaProvider::new(
            ollama,
            "nomic-embed-text",
            &settings.enrich_model,
        ));
        let local: Arc<dyn Generator> = Arc::new(OllamaGenerator::new(
            settings.chat_model.clone(),
            format!("{ollama}/v1"),
        ));
        let mut generators = vec![("local".to_string(), local, GENERATOR_MAX_OUTPUT_TOKENS)];
        let mut default_generator = "local".to_string();
        let mut judge = "local".to_string();
        if let Some(key) = &settings.anthropic_key {
            let claude: Arc<dyn Generator> =
                Arc::new(AnthropicGenerator::new(CLAUDE_MODEL, key.clone(), None));
            generators.push(("claude".to_string(), claude, GENERATOR_MAX_OUTPUT_TOKENS));
            default_generator = "claude".into();
            judge = "claude".into();
        }
        ModelDeps {
            embedder,
            // Dimension of nomic-embed-text; must match the embedding model above.
            embed_dimension: 768,
            enricher: Some(enricher),
            generators,
            default_generator,
            judge: Some(judge),
        }
    }
}

/// Wipes (unless `keep_data`) and rebuilds every store, then wires the engine.
/// This is the only place `AtlasState` is constructed.
pub async fn build_state(settings: Settings, models: ModelDeps) -> Result<AtlasState> {
    let dir = settings.data_dir.clone();
    if !settings.keep_data && dir.exists() {
        std::fs::remove_dir_all(&dir).with_context(|| format!("wipe {}", dir.display()))?;
    }
    std::fs::create_dir_all(&dir)?;
    let path = |name: &str| -> String { dir.join(name).to_string_lossy().into_owned() };

    let mut config = ArcanumConfig::from_file(Path::new("config.toml")).unwrap_or_default();
    config.retrieval.orchestration_mode = OrchestrationMode::ParallelFusion;
    config.generate.default_generator = Some(models.default_generator.clone());
    config.verify.judge = models.judge.clone();
    // `ingestion.pipeline` is not a config field: the pipeline template is chosen
    // per submission (`{"template":"full"}`), so there is nothing to set here.

    let vector_store = Arc::new(LanceDbStore::new(&path("atlas.lance")).await?);
    std::fs::create_dir_all(dir.join("bm25"))?;
    let bm25 = Arc::new(Bm25Index::new(&path("bm25"))?);
    let graph_store = Arc::new(InMemoryGraphStore::new());
    let tree_store = Arc::new(InMemoryTreeStore::new());
    let registry = Arc::new(InMemoryChunkMetadataStore::new());
    let version_store = Arc::new(SqliteDocumentVersionStore::open(&path("versions.db")).await?);
    let snapshot_store = Arc::new(LocalSnapshotStore::new(path("snapshots")));
    let evidence = Arc::new(DefaultEvidenceResolver::new(
        registry.clone(),
        version_store.clone(),
        Some(tree_store.clone()),
        Some(graph_store.clone()),
    ));

    // The only generator that speaks the Anthropic protocol is the optional "claude" one.
    let generator_meta: Vec<GeneratorMeta> = models
        .generators
        .iter()
        .map(|(name, g, _)| GeneratorMeta {
            name: name.clone(),
            protocol: if name == "claude" {
                "anthropic"
            } else {
                "openai-compatible"
            }
            .to_string(),
            model: g.model().to_string(),
            is_default: *name == models.default_generator,
        })
        .collect();
    let judge = models.judge.clone();

    let embed_dim = models.embed_dimension;
    let mut builder = ArcanumEngineBuilder::new(config)
        .auth_secret(&settings.auth_secret)
        .vector_store(vector_store.clone())
        .embedder(models.embedder)
        .graph_store(graph_store.clone())
        .tree_store(tree_store.clone())
        .version_store(version_store)
        .register_preprocessor("default", Arc::new(PassThroughPreprocessor))
        .snapshot_store(snapshot_store)
        .payload_store(Arc::new(LocalOperationPayloadStore::new(path("payloads"))))
        .bm25_index(bm25)
        .chunk_metadata_store(registry.clone())
        .evidence(evidence);
    if let Some(enricher) = models.enricher {
        builder = builder.enricher(enricher);
    }
    for (name, generator, max_tokens) in models.generators {
        builder = builder.generator(name, generator, max_tokens);
    }
    let engine = builder.build().await?;

    // Same store calls as POST /api/v1/{vector,graph,tree}/collections/:name.
    let ignore_exists = |r: arcanum_core::Result<()>| match r {
        Err(ArcanumError::AlreadyExists(_)) => Ok(()),
        other => other,
    };
    ignore_exists(vector_store.create_collection(COLLECTION).await)?;
    ignore_exists(graph_store.create_collection(COLLECTION).await)?;
    ignore_exists(tree_store.create_collection(COLLECTION).await)?;

    warm_up_vector_table(vector_store.as_ref(), embed_dim)
        .await
        .context("warm up the vector table")?;

    let admin_key = engine.auth.generate_admin_key("atlas");
    let claims = engine.auth.validate_api_key(&admin_key)?;
    let metrics_token: String = rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(32)
        .map(char::from)
        .collect();
    std::env::set_var("ARCANUM_METRICS_TOKEN", &metrics_token);

    let mcp = Arc::new(arcanum_mcp::McpJsonRpcHandler::new(engine.clone()));

    Ok(AtlasState {
        engine,
        registry,
        settings,
        admin_key,
        metrics_token,
        claims,
        generators: generator_meta,
        judge,
        mcp,
    })
}

/// `LanceDbStore::upsert` creates the collection's table on first use without a lock, so
/// concurrent first ingests race and some fail. Creating the table up front (one sentinel
/// chunk written straight to the vector store, then deleted) removes the race. The sentinel
/// never goes through ingestion, so it is not in the chunk registry, and it is gone from the
/// table before the engine serves anything.
async fn warm_up_vector_table(store: &dyn VectorStore, dim: usize) -> Result<()> {
    use arcanum_core::types::{
        Chunk, ChunkId, ChunkMetadata, ChunkPosition, ChunkProvenance, CollectionId, DocumentId,
        IndexedChunk, Vector,
    };
    if dim == 0 {
        anyhow::bail!("embed_dimension is 0; set the embedding dimension of the model in use");
    }
    let id = ChunkId::new();
    let chunk = Chunk {
        id: id.clone(),
        text: "atlas warm-up sentinel".into(),
        document_id: DocumentId::new(),
        collection_id: CollectionId(COLLECTION.into()),
        position: ChunkPosition {
            start: 0,
            end: 0,
            index: 0,
        },
        metadata: ChunkMetadata::default(),
        provenance: ChunkProvenance {
            document_version: 0,
            source_uri: "atlas-warm-up-sentinel".into(),
            snapshot_uri: String::new(),
            canonical_uri: None,
            page: None,
            section: None,
            block_ids: vec![],
        },
    };
    let indexed = IndexedChunk {
        chunk,
        vector: Vector(vec![0.0; dim]),
        token_vectors: None,
        store_id: id.0.to_string(),
    };
    store.upsert(COLLECTION, vec![indexed]).await?;
    store.delete(COLLECTION, &[id]).await?;
    Ok(())
}
