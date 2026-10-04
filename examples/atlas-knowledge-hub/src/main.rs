use anyhow::{Context, Result};
use atlas::demo::HttpOllamaProbe;
use atlas::samples::load_manifest;
use atlas::{assemble_app, build_state, ModelDeps, Settings};
use std::path::Path;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<()> {
    let settings = Settings::from_env();
    let models = ModelDeps::ollama(&settings);
    let state = Arc::new(build_state(settings, models).await?);
    let s = &state.settings;
    let manifest = Arc::new(load_manifest(Path::new("samples")).context("load samples")?);

    std::fs::write(".arcanum-dev-key", &state.admin_key).context("write .arcanum-dev-key")?;

    let mcp = arcanum_mcp::McpServer::new(state.mcp.clone(), s.mcp_port);
    tokio::spawn(async move {
        if let Err(e) = mcp.start().await {
            eprintln!("MCP server stopped: {e}");
        }
    });

    let probe = Arc::new(HttpOllamaProbe::new(&s.ollama_url));
    let app = assemble_app(state.clone(), manifest, probe);
    let listener = tokio::net::TcpListener::bind(format!("{}:{}", s.host, s.port))
        .await
        .with_context(|| format!("bind port {}", s.port))?;

    let ui = if Path::new(atlas::UI_DIST).is_dir() {
        format!("http://localhost:{}/", s.port)
    } else {
        "run make dev: http://localhost:5173".to_string()
    };
    println!("Atlas Knowledge Hub");
    println!("  API:      http://{}:{}", s.host, s.port);
    println!("  UI:       {ui}");
    println!(
        "  Bind:     {} (set ATLAS_HOST to widen; MCP binds all interfaces)",
        s.host
    );
    println!("  MCP:      http://localhost:{}/mcp", s.mcp_port);
    println!("  Ollama:   {}", s.ollama_url);
    println!(
        "  Models:   chat {}, embed nomic-embed-text, enrich {}",
        s.chat_model, s.enrich_model
    );
    for g in &state.generators {
        let mut marks = vec![];
        if g.is_default {
            marks.push("default");
        }
        if state.judge.as_deref() == Some(g.name.as_str()) {
            marks.push("judge");
        }
        let marks = if marks.is_empty() {
            String::new()
        } else {
            format!(" [{}]", marks.join(", "))
        };
        println!(
            "  Generator {} ({}, {}){marks}",
            g.name, g.protocol, g.model
        );
    }
    println!(
        "  Anthropic: {}",
        if s.anthropic_key.is_some() {
            "on"
        } else {
            "off"
        }
    );
    println!("  API key (admin, dev): {}", state.admin_key);

    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}
