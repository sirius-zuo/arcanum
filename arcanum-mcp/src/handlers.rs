use serde_json::{json, Value};
use arcanum_core::{Result, types::{Query, CollectionId}};
use arcanum_engine::{ArcanumEngine, IngestRequest, auth::ApiKeyClaims};
use std::sync::Arc;

/// System-level claims used for MCP calls (no per-user auth).
fn mcp_claims() -> ApiKeyClaims {
    ApiKeyClaims {
        user_id: "mcp".to_string(),
        allowed_collections: vec![],
        is_admin: true,
        exp: usize::MAX,
    }
}

pub struct McpJsonRpcHandler {
    engine: Option<Arc<ArcanumEngine>>,
}

impl McpJsonRpcHandler {
    pub fn new_test() -> Self { Self { engine: None } }

    pub fn new(engine: Arc<ArcanumEngine>) -> Self {
        Self { engine: Some(engine) }
    }

    pub async fn handle(&self, request: Value) -> Result<Value> {
        let id     = request.get("id").cloned().unwrap_or(Value::Null);
        let method = request["method"].as_str().unwrap_or("");

        match method {
            "initialize" => Ok(json!({
                "jsonrpc": "2.0", "id": id,
                "result": {
                    "protocolVersion": "2024-11-05",
                    "capabilities": { "tools": {}, "resources": {} },
                    "serverInfo": { "name": "arcanum", "version": "2.0.0" }
                }
            })),

            "tools/list" => Ok(json!({
                "jsonrpc": "2.0", "id": id,
                "result": {
                    "tools": [
                        { "name": "ingest",
                          "description": "Ingest a document into a collection",
                          "inputSchema": { "type": "object",
                            "properties": {
                              "source_uri": { "type": "string" },
                              "collection_id": { "type": "string" },
                              "pipeline": { "type": "string" }
                            }, "required": ["source_uri", "collection_id"] }},
                        { "name": "search",
                          "description": "Search a collection",
                          "inputSchema": { "type": "object",
                            "properties": {
                              "query": { "type": "string" },
                              "collection_id": { "type": "string" },
                              "top_k": { "type": "integer" }
                            }, "required": ["query", "collection_id"] }},
                        { "name": "list_collections",
                          "description": "List collections visible to the caller",
                          "inputSchema": { "type": "object", "properties": {} }},
                        { "name": "eval_run",
                          "description": "Run retrieval quality evaluation",
                          "inputSchema": { "type": "object",
                            "properties": { "collection_id": { "type": "string" } },
                            "required": ["collection_id"] }}
                    ]
                }
            })),

            "tools/call" => {
                let tool_name = request["params"]["name"].as_str().unwrap_or("");
                let args = &request["params"]["arguments"];
                self.dispatch_tool(id, tool_name, args).await
            }

            "resources/list" => Ok(json!({
                "jsonrpc": "2.0", "id": id,
                "result": { "resources": [] }
            })),

            _ => Ok(json!({
                "jsonrpc": "2.0", "id": id,
                "error": { "code": -32601, "message": "Method not found" }
            })),
        }
    }

    async fn dispatch_tool(&self, id: Value, name: &str, args: &Value) -> Result<Value> {
        match name {
            "ingest" => {
                let source_uri = args["source_uri"].as_str().unwrap_or("").to_string();
                let collection_id = args["collection_id"].as_str().unwrap_or("").to_string();
                let pipeline = args["pipeline"].as_str().map(|s| s.to_string());

                if let Some(engine) = &self.engine {
                    let req = IngestRequest {
                        source_uri,
                        collection_id: CollectionId(collection_id),
                        pipeline_template: pipeline,
                        force: false,
                    };
                    let op_id = engine.ingestion.ingest(req, "mcp").await?;
                    Ok(json!({
                        "jsonrpc": "2.0", "id": id,
                        "result": { "content": [{ "type": "text",
                            "text": format!("Ingestion queued. operation_id: {}", op_id.0) }] }
                    }))
                } else {
                    Ok(json!({
                        "jsonrpc": "2.0", "id": id,
                        "result": { "content": [{ "type": "text",
                            "text": format!("Ingestion queued. operation_id: {}", uuid::Uuid::new_v4()) }] }
                    }))
                }
            }
            "search" => {
                let query_text = args["query"].as_str().unwrap_or("").to_string();
                let collection_id = args["collection_id"].as_str().unwrap_or("").to_string();
                let top_k = args["top_k"].as_u64().unwrap_or(10) as usize;

                if let Some(engine) = &self.engine {
                    let query = Query::new(&query_text)
                        .with_collection(CollectionId(collection_id))
                        .with_top_k(top_k);
                    let claims = mcp_claims();
                    let result = engine.retrieval.search(query, &claims).await?;
                    let chunks_json = serde_json::to_string(&result.chunks).unwrap_or_default();
                    Ok(json!({
                        "jsonrpc": "2.0", "id": id,
                        "result": { "content": [{ "type": "text", "text": chunks_json }] }
                    }))
                } else {
                    Ok(json!({
                        "jsonrpc": "2.0", "id": id,
                        "result": { "content": [{ "type": "text", "text": "[]" }] }
                    }))
                }
            }
            "list_collections" => Ok(json!({
                "jsonrpc": "2.0", "id": id,
                "result": { "content": [{ "type": "text", "text": "[]" }] }
            })),
            _ => Ok(json!({
                "jsonrpc": "2.0", "id": id,
                "error": { "code": -32602, "message": format!("Unknown tool: {}", name) }
            })),
        }
    }
}
