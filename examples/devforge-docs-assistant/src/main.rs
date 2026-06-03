//! Devforge Docs Assistant
//!
//! Developer portal search — Standard ingestion, Static retrieval ([Vector, BM25]).
//!
//! Run:
//!   OLLAMA_URL=http://localhost:11434 cargo run
//!   Open: http://localhost:5173 (dev) or http://localhost:8080 (prod)
//!
//! See BUILD.md to switch to production stores.

use anyhow::Result;
use arcanum_core::config::ArcanumConfig;
use arcanum_engine::ArcanumEngineBuilder;
use arcanum_vector::LanceDbStore;
use arcanum_models::OllamaProvider;
use arcanum_server::build_app;
use std::sync::Arc;
use tokio::net::TcpListener;
use tower_http::services::{ServeDir, ServeFile};
use axum::Router;

#[tokio::main]
async fn main() -> Result<()> {
    let _telemetry = arcanum_telemetry::init(arcanum_telemetry::TelemetryConfig::from_env());

    let port     = std::env::var("PORT").unwrap_or_else(|_| "8080".into());
    let secret   = std::env::var("ARCANUM_AUTH_SECRET")
        .unwrap_or_else(|_| "arcanum-dev-secret-minimum-32chars!!".into());
    let ollama   = std::env::var("OLLAMA_URL")
        .unwrap_or_else(|_| "http://localhost:11434".into());

    // Load dev config (falls back to default if file not found)
    let config = ArcanumConfig::from_file(std::path::Path::new("config.toml"))
        .unwrap_or_default();

    std::fs::create_dir_all("data")?;

    // ── Vector store: LanceDB local file ─────────────────────────────────────
    // Production: PgVectorStore::new(&db_url, 768).await?  (see BUILD.md)
    let vector_store = Arc::new(LanceDbStore::new("data/devforge.lance").await?);

    // ── Embedder: Ollama local ────────────────────────────────────────────────
    // Production: HuggingFaceTeiProvider::new(&tei_url, "nomic-embed-text", 768)
    let embedder = Arc::new(OllamaProvider::new(&ollama, "nomic-embed-text", "nomic-embed-text"));

    let engine = ArcanumEngineBuilder::new(config)
        .auth_secret(&secret)
        .vector_store(vector_store)
        .embedder(embedder)
        .build()
        .await?;

    // Mint a real admin API key (signed with the engine secret) so the UI can call
    // the protected /api/v1/* routes. A fabricated string would fail validate_api_key.
    // Admin scope = access to any collection the user creates in the UI.
    let dev_key = engine.auth.generate_admin_key("dev-user");
    std::fs::write(".arcanum-dev-key", &dev_key)?;
    std::fs::write("ui/.env.development", format!("VITE_API_KEY={}\n", dev_key))?;

    // ── Build app, optionally serve built UI ─────────────────────────────────
    let mut app: Router = build_app(Some(engine));
    if std::path::Path::new("ui/dist").exists() {
        app = app.fallback_service(
            ServeDir::new("ui/dist")
                .fallback(ServeFile::new("ui/dist/index.html"))
        );
    }

    let addr = format!("0.0.0.0:{}", port);
    let listener = TcpListener::bind(&addr).await?;

    println!("┌────────────────────────────────────────────────┐");
    println!("│  Devforge Docs Assistant                        │");
    println!("│  API  → http://localhost:{port}                 │");
    if std::path::Path::new("ui/dist").exists() {
        println!("│  UI   → http://localhost:{port}                 │");
    } else {
        println!("│  UI   → http://localhost:5173  (run: make dev)  │");
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
