use axum::{routing::{get, post}, Router, Json, extract::State, http::HeaderMap, response::IntoResponse};
use std::sync::Arc;
use crate::handlers::McpJsonRpcHandler;
use serde_json::Value;
use tracing::debug;

pub struct McpServer {
    handler: Arc<McpJsonRpcHandler>,
    port: u16,
}

impl McpServer {
    pub fn new(handler: Arc<McpJsonRpcHandler>, port: u16) -> Self {
        Self { handler, port }
    }

    pub async fn start(self) -> Result<(), Box<dyn std::error::Error>> {
        debug!(port = self.port, "MCP server starting");
        let handler = self.handler.clone();
        let app = Router::new()
            .route("/mcp", post(handle_jsonrpc))
            .route("/health", get(|| async { "ok" }))
            .with_state(handler);
        let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", self.port)).await?;
        axum::serve(listener, app).await?;
        debug!(port = self.port, "MCP server stopped");
        Ok(())
    }
}

async fn handle_jsonrpc(
    State(handler): State<Arc<McpJsonRpcHandler>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> impl IntoResponse {
    match handler.handle(body, headers).await {
        Ok(resp) => Json(resp),
        Err(e) => Json(serde_json::json!({
            "jsonrpc": "2.0", "id": null,
            "error": { "code": -32603, "message": e.to_string() }
        })),
    }
}
