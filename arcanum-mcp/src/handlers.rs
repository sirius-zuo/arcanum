use serde_json::{json, Value};
use arcanum_core::Result;

pub struct McpJsonRpcHandler {}

impl McpJsonRpcHandler {
    pub fn new_test() -> Self { Self {} }

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

    async fn dispatch_tool(&self, id: Value, name: &str, _args: &Value) -> Result<Value> {
        match name {
            "ingest" => Ok(json!({
                "jsonrpc": "2.0", "id": id,
                "result": { "content": [{ "type": "text",
                    "text": format!("Ingestion queued. operation_id: {}", uuid::Uuid::new_v4()) }] }
            })),
            "search" => Ok(json!({
                "jsonrpc": "2.0", "id": id,
                "result": { "content": [{ "type": "text", "text": "[]" }] }
            })),
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
