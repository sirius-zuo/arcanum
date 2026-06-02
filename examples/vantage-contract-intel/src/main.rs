//! Vantage Contract Intelligence
//!
//! Contract portfolio search — FULL ingestion (entity graph + RAPTOR + contextual
//! enrichment), ParallelFusion retrieval. All four strategies run per query; RRF
//! fuses them so exact terms, semantics, party entities, and clause summaries all
//! contribute.
//!
//! Dev uses in-memory graph + tree stores and an Ollama enricher. See BUILD.md for
//! production stores.
//!
//! Run:
//!   OLLAMA_URL=http://localhost:11434 cargo run
//!   Open: http://localhost:5173 (dev) or http://localhost:8080 (prod)

use anyhow::Result;
use arcanum_core::config::ArcanumConfig;
use arcanum_engine::ArcanumEngineBuilder;
use arcanum_graph::InMemoryGraphStore;
use arcanum_models::OllamaProvider;
use arcanum_server::build_app;
use arcanum_tree::InMemoryTreeStore;
use arcanum_vector::LanceDbStore;
use axum::Router;
use std::sync::Arc;
use tokio::net::TcpListener;
use tower_http::services::{ServeDir, ServeFile};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let port   = std::env::var("PORT").unwrap_or_else(|_| "8080".into());
    let secret = std::env::var("ARCANUM_AUTH_SECRET")
        .unwrap_or_else(|_| "arcanum-dev-secret-minimum-32chars!!".into());
    let ollama = std::env::var("OLLAMA_URL")
        .unwrap_or_else(|_| "http://localhost:11434".into());

    let config = ArcanumConfig::from_file(std::path::Path::new("config.toml"))
        .unwrap_or_default();

    std::fs::create_dir_all("data")?;

    // ── Vector store: LanceDB local file ─────────────────────────────────────
    // Production: PgVectorStore::new(&db_url, 768).await?  (see BUILD.md)
    let vector_store = Arc::new(LanceDbStore::new("data/vantage.lance").await?);

    // ── Embedder: Ollama local ────────────────────────────────────────────────
    // Production: HuggingFaceTeiProvider::new(&tei_url, "nomic-embed-text", 768)
    let embedder = Arc::new(OllamaProvider::new(&ollama, "nomic-embed-text", "nomic-embed-text"));

    // ── Enricher: Ollama local (ExtractEntities, ContextPrefix, Summarize) ────
    // Production: EnrichmentDispatcher::new(claude).with_override(ExtractEntities, gliner)
    let enricher = Arc::new(OllamaProvider::new(&ollama, "nomic-embed-text", "qwen2.5"));

    // ── Graph store: in-memory (dev) ──────────────────────────────────────────
    // Production: Neo4jStore::new(&neo4j_url, &user, &password).await?
    let graph_store = Arc::new(InMemoryGraphStore::new());

    // ── Tree store: in-memory (dev) ───────────────────────────────────────────
    // Production: PgTreeStore::new(&db_url).await?
    let tree_store = Arc::new(InMemoryTreeStore::new());

    let engine = ArcanumEngineBuilder::new(config)
        .auth_secret(&secret)
        .vector_store(vector_store)
        .embedder(embedder)
        .enricher(enricher)
        .graph_store(graph_store)
        .tree_store(tree_store)
        .build()
        .await?;

    // Real admin API key (signed JWT) — a fabricated string would fail validate_api_key.
    let dev_key = engine.auth.generate_admin_key("dev-user");
    std::fs::write(".arcanum-dev-key", &dev_key)?;
    std::fs::write("ui/.env.development", format!("VITE_API_KEY={}\n", dev_key))?;

    // /api/v1/graph is served by arcanum-server (the engine exposes graph_store).
    let mut app: Router = build_app(Some(engine));

    if std::path::Path::new("ui/dist").exists() {
        app = app.fallback_service(
            ServeDir::new("ui/dist").fallback(ServeFile::new("ui/dist/index.html")),
        );
    }

    let listener = TcpListener::bind(format!("0.0.0.0:{}", port)).await?;

    println!("┌────────────────────────────────────────────────┐");
    println!("│  Vantage Contract Intelligence                  │");
    println!("│  API   → http://localhost:{port}                │");
    println!("│  Graph → http://localhost:{port}/api/v1/graph   │");
    if std::path::Path::new("ui/dist").exists() {
        println!("│  UI    → http://localhost:{port}                │");
    } else {
        println!("│  UI    → http://localhost:5173 (run: make dev) │");
    }
    println!("└────────────────────────────────────────────────┘");
    println!("  API key (admin, dev): {dev_key}");

    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            tokio::signal::ctrl_c().await.unwrap();
            println!("\nShutting down…");
        })
        .await?;

    Ok(())
}
