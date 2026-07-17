use axum::http::HeaderMap;
use serde_json::{json, Value};
use arcanum_core::{Result, types::{Query, CollectionId}};
use arcanum_engine::{ArcanumEngine, IngestRequest, auth::ApiKeyClaims};
use crate::capability_registry::{CapabilityRegistry, ToolDefinition};
use std::sync::Arc;
use tracing::instrument;
use metrics;

pub struct McpJsonRpcHandler {
    engine: Option<Arc<ArcanumEngine>>,
    registry: Arc<CapabilityRegistry>,
}

impl McpJsonRpcHandler {
    pub fn new_test() -> Self {
        Self {
            engine: None,
            registry: Self::default_registry(),
        }
    }

    pub fn new(engine: Arc<ArcanumEngine>) -> Self {
        Self {
            engine: Some(engine),
            registry: Self::default_registry(),
        }
    }

    fn default_registry() -> Arc<CapabilityRegistry> {
        let registry = CapabilityRegistry::new();
        registry.register(ToolDefinition::new(
            "ingest", "Ingest a document into a collection",
            json!({ "type": "object",
                "properties": {
                    "source_uri": { "type": "string" },
                    "collection_id": { "type": "string" },
                    "pipeline": { "type": "string" }
                }, "required": ["source_uri", "collection_id"] }),
        ));
        registry.register(ToolDefinition::new(
            "search", "Search a collection",
            json!({ "type": "object",
                "properties": {
                    "query": { "type": "string" },
                    "collection_id": { "type": "string" },
                    "top_k": { "type": "integer" }
                }, "required": ["query", "collection_id"] }),
        ));
        registry.register(ToolDefinition::new(
            "list_collections", "List collections visible to the caller",
            json!({ "type": "object", "properties": {} }),
        ));
        registry.register(ToolDefinition::new(
            "eval_run", "Run retrieval quality evaluation against a collection using caller-supplied golden samples",
            json!({ "type": "object",
                "properties": {
                    "collection_id": { "type": "string" },
                    "samples": { "type": "array", "items": { "type": "object",
                        "properties": {
                            "query": { "type": "string" },
                            "relevant_chunk_ids": { "type": "array", "items": { "type": "string" } }
                        }, "required": ["query", "relevant_chunk_ids"] } },
                    "k": { "type": "integer", "default": 5 }
                }, "required": ["collection_id", "samples"] }),
        ));
        Arc::new(registry)
    }

    fn extract_claims(&self, headers: &HeaderMap) -> std::result::Result<ApiKeyClaims, Value> {
        let engine = self.engine.as_ref().ok_or_else(|| json!({
            "jsonrpc": "2.0", "id": serde_json::Value::Null,
            "error": { "code": -32001, "message": "engine not initialised" }
        }))?;
        let token = headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.strip_prefix("Bearer "))
            .ok_or_else(|| json!({
                "jsonrpc": "2.0", "id": serde_json::Value::Null,
                "error": { "code": -32001, "message": "missing Authorization header" }
            }))?;
        engine.auth.validate_api_key(token).map_err(|_| json!({
            "jsonrpc": "2.0", "id": serde_json::Value::Null,
            "error": { "code": -32001, "message": "invalid or expired token" }
        }))
    }

    #[instrument(skip(self, request, headers), fields(method = extract_method(&request)))]
    pub async fn handle(&self, request: Value, headers: HeaderMap) -> Result<Value> {
        let start = std::time::Instant::now();
        let id     = request.get("id").cloned().unwrap_or(Value::Null);
        let method = extract_method(&request);

        let result = match method {
            "initialize" => Ok(json!({
                "jsonrpc": "2.0", "id": id,
                "result": {
                    "protocolVersion": "2024-11-05",
                    "capabilities": { "tools": {}, "resources": {} },
                    "serverInfo": { "name": "arcanum", "version": "2.0.0" }
                }
            })),

            "tools/list" => {
                let tools: Vec<Value> = self.registry.list().iter().map(|t| json!({
                    "name": t.name, "description": t.description, "inputSchema": t.input_schema,
                })).collect();
                Ok(json!({ "jsonrpc": "2.0", "id": id, "result": { "tools": tools } }))
            }

            "tools/call" => {
                let claims = match self.extract_claims(&headers) {
                    Ok(c) => c,
                    Err(err_resp) => return Ok(err_resp),
                };
                let tool_name = request["params"]["name"].as_str().unwrap_or("");
                let args = &request["params"]["arguments"];
                self.dispatch_tool(id, tool_name, args, &claims).await
            }

            "resources/list" => Ok(json!({
                "jsonrpc": "2.0", "id": id,
                "result": { "resources": [] }
            })),

            _ => Ok(json!({
                "jsonrpc": "2.0", "id": id,
                "error": { "code": -32601, "message": "Method not found" }
            })),
        };
        let elapsed = start.elapsed().as_secs_f64();
        let status = if result.is_ok() { "ok" } else { "error" };
        metrics::counter!("arcanum_mcp_requests_total", "method" => method.to_string(), "status" => status).increment(1);
        metrics::histogram!("arcanum_mcp_request_duration_seconds", "method" => method.to_string()).record(elapsed);
        result
    }

    async fn dispatch_tool(
        &self,
        id: Value,
        name: &str,
        args: &Value,
        claims: &ApiKeyClaims,
    ) -> Result<Value> {
        match name {
            "ingest" => {
                let source_uri    = args["source_uri"].as_str().unwrap_or("").to_string();
                let collection_id = args["collection_id"].as_str().unwrap_or("").to_string();
                let pipeline      = args["pipeline"].as_str().map(|s| s.to_string());

                if let Some(engine) = &self.engine {
                    let req = IngestRequest {
                        source_uri,
                        collection_id: CollectionId(collection_id),
                        pipeline_template: pipeline,
                        force: false,
                        content: None,
                        mime_hint: None,
                    };
                    let op_id = engine.ingestion.ingest(req, &claims.user_id).await?;
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
                let query_text    = args["query"].as_str().unwrap_or("").to_string();
                let collection_id = args["collection_id"].as_str().unwrap_or("").to_string();
                let top_k         = args["top_k"].as_u64().unwrap_or(10) as usize;

                if let Some(engine) = &self.engine {
                    let query = Query::new(&query_text)
                        .with_collection(CollectionId(collection_id))
                        .with_top_k(top_k);
                    let result = engine.retrieval.search(query, claims).await?;
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
            "list_collections" => {
                if let Some(engine) = &self.engine {
                    let cols = engine.version_store.list_collections().await?;
                    let visible: Vec<String> = cols.into_iter()
                        .filter(|c| engine.auth.can_access_collection(claims, c))
                        .collect();
                    let text = serde_json::to_string(&visible).unwrap_or_else(|_| "[]".into());
                    Ok(json!({
                        "jsonrpc": "2.0", "id": id,
                        "result": { "content": [{ "type": "text", "text": text }] }
                    }))
                } else {
                    Ok(json!({
                        "jsonrpc": "2.0", "id": id,
                        "result": { "content": [{ "type": "text", "text": "[]" }] }
                    }))
                }
            }
            _ => Ok(json!({
                "jsonrpc": "2.0", "id": id,
                "error": { "code": -32602, "message": format!("Unknown tool: {}", name) }
            })),
        }
    }
}

fn extract_method(request: &Value) -> &str {
    request.get("method").and_then(|v| v.as_str()).unwrap_or("unknown")
}

#[cfg(test)]
mod tests {
    use super::*;
    use arcanum_engine::ArcanumEngine;

    async fn test_engine() -> Arc<ArcanumEngine> {
        ArcanumEngine::builder()
            .auth_secret("a-32-char-secret-for-testing-ok!")
            .version_store(Arc::new(arcanum_core::traits::NoOpDocumentVersionStore))
            .build()
            .await
            .expect("engine build should succeed")
    }

    fn make_headers(token: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::AUTHORIZATION,
            format!("Bearer {}", token).parse().unwrap(),
        );
        headers
    }

    fn no_headers() -> HeaderMap { HeaderMap::new() }

    /// Copied verbatim from `NoOpDocumentVersionStore`'s impl, except
    /// `list_collections` returns fixtures instead of an empty vec.
    struct FakeVersionStore;

    #[async_trait::async_trait]
    impl arcanum_core::traits::DocumentVersionStore for FakeVersionStore {
        async fn get_latest(&self, _: &str, _: &str) -> Result<Option<arcanum_core::types::DocumentVersion>> {
            Ok(None)
        }
        async fn add_version(&self, _: arcanum_core::types::DocumentVersion) -> Result<()> { Ok(()) }
        async fn supersede_active(&self, _: &arcanum_core::types::DocumentId) -> Result<()> { Ok(()) }
        async fn list_versions(&self, _: &arcanum_core::types::DocumentId) -> Result<Vec<arcanum_core::types::DocumentVersion>> { Ok(vec![]) }
        async fn get_versioning_policy(&self, _: &str) -> Result<arcanum_core::types::VersioningPolicy> {
            Ok(arcanum_core::types::VersioningPolicy::Replace)
        }
        async fn set_versioning_policy(&self, _: &str, _: arcanum_core::types::VersioningPolicy) -> Result<()> { Ok(()) }
        async fn delete_by_source_uri(&self, _: &str, _: &str) -> Result<()> { Ok(()) }
        async fn list_collections(&self) -> Result<Vec<String>> {
            Ok(vec!["col1".into(), "col2".into()])
        }
        async fn get_version(&self, _: &arcanum_core::types::DocumentId, _: u32) -> Result<Option<arcanum_core::types::DocumentVersion>> {
            Ok(None)
        }
        async fn list_documents(&self, _: &str) -> Result<Vec<arcanum_core::types::DocumentEntry>> {
            Ok(vec![])
        }
    }

    #[tokio::test]
    async fn test_list_collections_filters_by_claims() {
        let engine = ArcanumEngine::builder()
            .auth_secret("a-32-char-secret-for-testing-ok!")
            .version_store(Arc::new(FakeVersionStore))
            .build().await.unwrap();
        // Token scoped to col1 only.
        let token = engine.auth.generate_api_key("user1", vec!["col1".into()]);
        let handler = McpJsonRpcHandler::new(engine);
        let req = json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call",
            "params": { "name": "list_collections", "arguments": {} } });
        let resp = handler.handle(req, make_headers(&token)).await.unwrap();
        let text = resp["result"]["content"][0]["text"].as_str().unwrap();
        let cols: Vec<String> = serde_json::from_str(text).unwrap();
        assert_eq!(cols, vec!["col1"], "must include accessible col1 and exclude col2");
    }

    #[tokio::test]
    async fn test_tools_call_without_auth_returns_32001() {
        let engine = test_engine().await;
        let handler = McpJsonRpcHandler::new(engine);
        let req = json!({
            "jsonrpc": "2.0", "id": 1,
            "method": "tools/call",
            "params": { "name": "search", "arguments": { "query": "test", "collection_id": "col1" } }
        });
        let resp = handler.handle(req, no_headers()).await.unwrap();
        assert_eq!(resp["error"]["code"], -32001,
            "unauthenticated tools/call should return -32001");
    }

    #[tokio::test]
    async fn test_initialize_without_auth_succeeds() {
        let engine = test_engine().await;
        let handler = McpJsonRpcHandler::new(engine);
        let req = json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {} });
        let resp = handler.handle(req, no_headers()).await.unwrap();
        assert!(resp["error"].is_null(), "initialize should not require auth");
        assert_eq!(resp["result"]["serverInfo"]["name"], "arcanum");
    }

    #[tokio::test]
    async fn test_tools_call_with_valid_token_dispatches() {
        let engine = test_engine().await;
        let token = engine.auth.generate_api_key("user1", vec!["col1".into()]);
        let handler = McpJsonRpcHandler::new(engine);
        let req = json!({
            "jsonrpc": "2.0", "id": 1,
            "method": "tools/call",
            "params": { "name": "search", "arguments": { "query": "test", "collection_id": "col1" } }
        });
        let resp = handler.handle(req, make_headers(&token)).await.unwrap();
        assert_ne!(resp["error"]["code"], -32001, "valid token should not get auth error");
    }

    #[tokio::test]
    async fn test_tools_list_is_driven_by_registry() {
        let engine = test_engine().await;
        let handler = McpJsonRpcHandler::new(engine);
        let req = json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list", "params": {} });
        let resp = handler.handle(req, no_headers()).await.unwrap();
        let tools = resp["result"]["tools"].as_array().expect("tools array");
        let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
        // CapabilityRegistry::list() sorts by name.
        assert_eq!(names, vec!["eval_run", "ingest", "list_collections", "search"]);
        // Every tool must carry a schema — proves we serialized ToolDefinition, not a stub.
        for t in tools {
            assert!(t["inputSchema"].is_object(), "tool {} missing inputSchema", t["name"]);
        }
    }
}
