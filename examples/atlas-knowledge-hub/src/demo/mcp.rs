use super::{require_key, DemoCtx, DemoError};
use axum::extract::State;
use axum::http::HeaderMap;
use axum::Json;
use serde_json::{json, Value};

/// `GET /demo/mcp`: the MCP endpoint URL and its tool catalogue (from `tools/list`).
pub async fn mcp(State(ctx): State<DemoCtx>, headers: HeaderMap) -> Result<Json<Value>, DemoError> {
    require_key(&ctx.state, &headers)?;
    let resp = ctx
        .state
        .mcp
        .handle(
            json!({"jsonrpc":"2.0","id":1,"method":"tools/list"}),
            HeaderMap::new(),
        )
        .await
        .map_err(|e| DemoError::Internal(format!("mcp tools/list failed: {e}")))?;
    let tools: Vec<Value> = resp["result"]["tools"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|t| {
            json!({
                "name": t["name"],
                "description": t["description"],
                "input_schema": t["inputSchema"],
            })
        })
        .collect();
    Ok(Json(json!({
        "endpoint": format!("http://localhost:{}/mcp", ctx.state.settings.mcp_port),
        "tools": tools,
    })))
}
