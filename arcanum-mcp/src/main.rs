use arcanum_engine::ArcanumEngine;
use arcanum_ingestion::SqliteDocumentVersionStore;
use arcanum_mcp::{handlers::McpJsonRpcHandler, server::McpServer};
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let secret = std::env::var("ARCANUM_AUTH_SECRET")
        .map_err(|_| "ARCANUM_AUTH_SECRET is required (min 32 chars)")?;
    let port: u16 = std::env::var("MCP_PORT").ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8081);
    let db_path = std::env::var("ARCANUM_DB_PATH")
        .unwrap_or_else(|_| "./arcanum-mcp.db".into());

    let engine = ArcanumEngine::builder()
        .auth_secret(&secret)
        .version_store(Arc::new(SqliteDocumentVersionStore::open(&db_path).await?))
        .build()
        .await?;

    tracing::info!(port, db_path, "arcanum-mcp starting (no embedder/vector store configured — \
        search and ingest tools will error until wired; see examples/ for full wiring)");
    McpServer::new(Arc::new(McpJsonRpcHandler::new(engine)), port).start().await
}
