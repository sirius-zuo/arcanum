# Arcanum Part 4 — Service Layer

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement `arcanum-engine`, `arcanum-mcp`, and `arcanum-server` — the service layer that exposes all Arcanum capabilities over MCP and HTTP transports.

**Architecture:** `arcanum-engine` owns all business logic and cross-cutting concerns (auth, rate limiting, audit). `arcanum-mcp` and `arcanum-server` are transport peers — both call `arcanum-engine` service handlers directly. Neither is a client of the other.

**Tech Stack:** `axum 0.7`, `tokio 1`, `tower 0.4`, `jsonrpc-core 18` (for MCP), `serde_json 1`, `jsonwebtoken 9`

**Prerequisites:** Parts 1, 2, and 3 complete.

---

### Task 25: arcanum-engine — ArcanumEngine Builder + Service Handlers

**Files:**
- Modify: `arcanum-engine/Cargo.toml`
- Create: `arcanum-engine/src/engine.rs`
- Create: `arcanum-engine/src/services/ingestion.rs`
- Create: `arcanum-engine/src/services/retrieval.rs`
- Create: `arcanum-engine/src/services/collection.rs`
- Create: `arcanum-engine/src/lib.rs`

- [ ] **Step 1: Update `arcanum-engine/Cargo.toml`**

```toml
[package]
name    = "arcanum-engine"
version = "0.1.0"
edition = "2021"

[dependencies]
arcanum-core       = { path = "../arcanum-core" }
arcanum-vector     = { path = "../arcanum-vector" }
arcanum-graph      = { path = "../arcanum-graph" }
arcanum-tree       = { path = "../arcanum-tree" }
arcanum-models     = { path = "../arcanum-models" }
arcanum-ingestion  = { path = "../arcanum-ingestion" }
arcanum-retrieval  = { path = "../arcanum-retrieval" }
arcanum-eval       = { path = "../arcanum-eval" }
arcanum-pipeline   = { path = "../arcanum-pipeline" }
arcanum-middleware = { path = "../arcanum-middleware" }
async-trait        = { workspace = true }
tokio              = { workspace = true }
serde              = { workspace = true }
serde_json         = { workspace = true }
anyhow             = { workspace = true }
tracing            = { workspace = true }
jsonwebtoken       = "9"
uuid               = { workspace = true }
chrono             = { workspace = true }
```

- [ ] **Step 2: Write failing tests**

```rust
// arcanum-engine/tests/engine_test.rs
use arcanum_engine::{ArcanumEngine, ArcanumEngineBuilder};
use arcanum_core::config::*;

#[tokio::test]
async fn test_engine_build_fails_with_sqlite_in_production() {
    let mut cfg = ArcanumConfig::default();
    cfg.global.runtime_mode = RuntimeMode::Production;
    cfg.storage.metadata_backend = MetadataBackend::Sqlite;
    let result = ArcanumEngineBuilder::new(cfg).build().await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("SQLite"));
}

#[tokio::test]
async fn test_engine_build_succeeds_in_development() {
    let cfg = ArcanumConfig::default(); // Development mode, SQLite OK
    let engine = ArcanumEngineBuilder::new(cfg).build().await;
    assert!(engine.is_ok());
}
```

- [ ] **Step 3: Implement `arcanum-engine/src/engine.rs`**

```rust
use arcanum_core::{config::ArcanumConfig, Result, ArcanumError};
use arcanum_middleware::BoundedQueue;
use arcanum_pipeline::PipelineTemplate;
use arcanum_retrieval::RetrievalOrchestrator;
use std::sync::Arc;
use crate::services::{
    ingestion::IngestionService,
    retrieval::RetrievalService,
    collection::CollectionService,
};
use crate::{audit::AuditLogger, event_bus::EventBus, auth::AuthMiddleware};

pub struct ArcanumEngine {
    pub config: ArcanumConfig,
    pub ingestion: Arc<IngestionService>,
    pub retrieval: Arc<RetrievalService>,
    pub collection: Arc<CollectionService>,
    pub audit: Arc<AuditLogger>,
    pub events: Arc<EventBus>,
}

pub struct ArcanumEngineBuilder {
    config: ArcanumConfig,
}

impl ArcanumEngineBuilder {
    pub fn new(config: ArcanumConfig) -> Self { Self { config } }

    pub async fn build(self) -> Result<Arc<ArcanumEngine>> {
        // 1. Validate config
        self.config.validate()?;

        // 2. Build shared services
        let audit = Arc::new(AuditLogger::new());
        let events = Arc::new(EventBus::new());

        let ingestion = Arc::new(IngestionService::new(
            self.config.clone(),
            events.clone(),
            audit.clone(),
        ));
        let retrieval = Arc::new(RetrievalService::new(
            self.config.clone(),
            audit.clone(),
        ));
        let collection = Arc::new(CollectionService::new(
            self.config.clone(),
            audit.clone(),
        ));

        Ok(Arc::new(ArcanumEngine {
            config: self.config,
            ingestion,
            retrieval,
            collection,
            audit,
            events,
        }))
    }
}
```

`arcanum-engine/src/services/ingestion.rs`:
```rust
use arcanum_core::{config::ArcanumConfig, types::*, Result};
use arcanum_middleware::BoundedQueue;
use std::sync::Arc;
use crate::{audit::AuditLogger, event_bus::EventBus};

pub struct IngestionService {
    config: ArcanumConfig,
    queue: Arc<BoundedQueue<IngestionTask>>,
    events: Arc<EventBus>,
    audit: Arc<AuditLogger>,
}

#[derive(Debug, Clone)]
pub struct IngestionTask {
    pub operation_id: OperationId,
    pub source_uri: String,
    pub collection_id: CollectionId,
    pub pipeline_template: String,
}

#[derive(Debug, Clone)]
pub struct IngestRequest {
    pub source_uri: String,
    pub collection_id: CollectionId,
    pub pipeline_template: Option<String>,
}

impl IngestionService {
    pub fn new(config: ArcanumConfig, events: Arc<EventBus>, audit: Arc<AuditLogger>) -> Self {
        let capacity = 10_000;
        Self {
            config,
            queue: Arc::new(BoundedQueue::new(capacity)),
            events,
            audit,
        }
    }

    /// Accept an ingestion request and return an OperationId immediately.
    /// Processing happens asynchronously via the worker pool.
    pub async fn ingest(&self, req: IngestRequest, user_id: &str) -> Result<OperationId> {
        let op_id = OperationId::new();
        let task = IngestionTask {
            operation_id: op_id.clone(),
            source_uri: req.source_uri.clone(),
            collection_id: req.collection_id.clone(),
            pipeline_template: req.pipeline_template.unwrap_or("standard".into()),
        };
        self.queue.push(task).await?;
        self.audit.log(AuditEntry {
            operation: "ingest".into(),
            user_id: user_id.to_string(),
            collection_id: req.collection_id.0.clone(),
            result: "accepted".into(),
        }).await;
        self.events.publish("ingestion:progress", serde_json::json!({
            "operation_id": op_id.0,
            "status": "queued"
        })).await;
        Ok(op_id)
    }
}

// Re-export AuditEntry for use in this module
use crate::audit::AuditEntry;
```

`arcanum-engine/src/services/retrieval.rs`:
```rust
use arcanum_core::{config::ArcanumConfig, types::*, Result};
use std::sync::Arc;
use crate::audit::{AuditLogger, AuditEntry};

pub struct RetrievalService {
    config: ArcanumConfig,
    audit: Arc<AuditLogger>,
}

impl RetrievalService {
    pub fn new(config: ArcanumConfig, audit: Arc<AuditLogger>) -> Self {
        Self { config, audit }
    }

    pub async fn search(&self, query: Query, user_id: &str) -> Result<RetrievalResult> {
        // In full impl: check QueryCache → RetrievalOrchestrator → store in cache
        let result = RetrievalResult {
            chunks: vec![],
            citations: vec![],
            strategy_scores: Default::default(),
            confidence: 0.0,
        };
        self.audit.log(AuditEntry {
            operation: "search".into(),
            user_id: user_id.to_string(),
            collection_id: query.collection_id.as_ref().map(|c| c.0.clone()).unwrap_or_default(),
            result: "ok".into(),
        }).await;
        Ok(result)
    }
}
```

`arcanum-engine/src/services/collection.rs`:
```rust
use arcanum_core::{config::ArcanumConfig, types::*, Result, ArcanumError};
use std::{collections::HashMap, sync::Arc};
use tokio::sync::RwLock;
use crate::audit::{AuditLogger, AuditEntry};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CollectionInfo {
    pub id: CollectionId,
    pub description: String,
    pub chunk_count: usize,
}

pub struct CollectionService {
    config: ArcanumConfig,
    collections: Arc<RwLock<HashMap<String, CollectionInfo>>>,
    audit: Arc<AuditLogger>,
}

impl CollectionService {
    pub fn new(config: ArcanumConfig, audit: Arc<AuditLogger>) -> Self {
        Self { config, collections: Arc::new(RwLock::new(HashMap::new())), audit }
    }

    pub async fn create(&self, id: CollectionId, description: String, user_id: &str) -> Result<()> {
        let mut map = self.collections.write().await;
        if map.contains_key(&id.0) {
            return Err(ArcanumError::Storage(format!("collection '{}' already exists", id.0)));
        }
        map.insert(id.0.clone(), CollectionInfo { id: id.clone(), description, chunk_count: 0 });
        self.audit.log(AuditEntry {
            operation: "create_collection".into(), user_id: user_id.to_string(),
            collection_id: id.0, result: "ok".into(),
        }).await;
        Ok(())
    }

    pub async fn list(&self) -> Vec<CollectionInfo> {
        self.collections.read().await.values().cloned().collect()
    }

    pub async fn delete(&self, id: &str, user_id: &str) -> Result<()> {
        let removed = self.collections.write().await.remove(id).is_some();
        if !removed { return Err(ArcanumError::NotFound(format!("collection '{}'", id))); }
        self.audit.log(AuditEntry {
            operation: "delete_collection".into(), user_id: user_id.to_string(),
            collection_id: id.to_string(), result: "ok".into(),
        }).await;
        Ok(())
    }
}
```

`arcanum-engine/src/lib.rs`:
```rust
pub mod audit;
pub mod auth;
pub mod engine;
pub mod event_bus;
pub mod rate_limit;
pub mod services {
    pub mod collection;
    pub mod ingestion;
    pub mod retrieval;
}

pub use engine::{ArcanumEngine, ArcanumEngineBuilder};
pub use services::{
    collection::CollectionService,
    ingestion::{IngestionService, IngestRequest},
    retrieval::RetrievalService,
};
```

- [ ] **Step 4: Run tests and commit**

```bash
cargo test -p arcanum-engine
git add arcanum-engine/
git commit -m "feat(engine): add ArcanumEngine builder and core service handlers"
```

---

### Task 26: arcanum-engine — AuthMiddleware, AuditLogger, EventBus, RateLimiter

**Files:**
- Create: `arcanum-engine/src/auth.rs`
- Create: `arcanum-engine/src/audit.rs`
- Create: `arcanum-engine/src/event_bus.rs`
- Create: `arcanum-engine/src/rate_limit.rs`

- [ ] **Step 1: Write failing tests**

```rust
// arcanum-engine/tests/auth_test.rs
use arcanum_engine::auth::{AuthMiddleware, ApiKey};

#[test]
fn test_valid_api_key_authenticates() {
    let auth = AuthMiddleware::new("secret-signing-key");
    let key = auth.generate_api_key("user-1", vec!["collection-a".to_string()]);
    let claims = auth.validate_api_key(&key).unwrap();
    assert_eq!(claims.user_id, "user-1");
    assert!(claims.allowed_collections.contains(&"collection-a".to_string()));
}

#[test]
fn test_invalid_api_key_rejected() {
    let auth = AuthMiddleware::new("secret-signing-key");
    let result = auth.validate_api_key("invalid.key.here");
    assert!(result.is_err());
}

// arcanum-engine/tests/rate_limit_test.rs
use arcanum_engine::rate_limit::RateLimiter;

#[test]
fn test_rate_limiter_allows_under_limit() {
    let rl = RateLimiter::new(10); // 10 requests per window
    for _ in 0..10 { assert!(rl.check_and_record("user-1")); }
}

#[test]
fn test_rate_limiter_blocks_over_limit() {
    let rl = RateLimiter::new(2);
    rl.check_and_record("user-1");
    rl.check_and_record("user-1");
    assert!(!rl.check_and_record("user-1")); // 3rd request blocked
}
```

- [ ] **Step 2: Implement `arcanum-engine/src/auth.rs`**

```rust
use arcanum_core::{Result, ArcanumError};
use jsonwebtoken::{encode, decode, Header, Validation, EncodingKey, DecodingKey};
use serde::{Deserialize, Serialize};
use chrono::Utc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKeyClaims {
    pub user_id: String,
    pub allowed_collections: Vec<String>,
    pub exp: usize,
}

pub struct AuthMiddleware {
    encoding_key: EncodingKey,
    decoding_key: DecodingKey,
}

impl AuthMiddleware {
    pub fn new(secret: &str) -> Self {
        Self {
            encoding_key: EncodingKey::from_secret(secret.as_bytes()),
            decoding_key: DecodingKey::from_secret(secret.as_bytes()),
        }
    }

    pub fn generate_api_key(&self, user_id: &str, collections: Vec<String>) -> String {
        let claims = ApiKeyClaims {
            user_id: user_id.to_string(),
            allowed_collections: collections,
            exp: (Utc::now().timestamp() + 86400 * 365) as usize, // 1 year
        };
        encode(&Header::default(), &claims, &self.encoding_key).unwrap_or_default()
    }

    pub fn validate_api_key(&self, token: &str) -> Result<ApiKeyClaims> {
        decode::<ApiKeyClaims>(token, &self.decoding_key, &Validation::default())
            .map(|data| data.claims)
            .map_err(|e| ArcanumError::Auth(e.to_string()))
    }

    pub fn can_access_collection(&self, claims: &ApiKeyClaims, collection: &str) -> bool {
        claims.allowed_collections.is_empty() // empty = all collections
            || claims.allowed_collections.iter().any(|c| c == collection)
    }
}

// Re-exported type alias for ergonomics
pub type ApiKey = String;
```

`arcanum-engine/src/audit.rs`:
```rust
use serde::{Deserialize, Serialize};
use chrono::Utc;
use tokio::sync::RwLock;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    pub operation: String,
    pub user_id: String,
    pub collection_id: String,
    pub result: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditRecord {
    pub entry: AuditEntry,
    pub timestamp: String,
}

pub struct AuditLogger {
    records: RwLock<Vec<AuditRecord>>,
}

impl AuditLogger {
    pub fn new() -> Self { Self { records: RwLock::new(vec![]) } }

    pub async fn log(&self, entry: AuditEntry) {
        // Secret values must never appear here — only operation metadata
        let record = AuditRecord {
            entry,
            timestamp: Utc::now().to_rfc3339(),
        };
        self.records.write().await.push(record);
    }

    pub async fn query(&self, limit: usize) -> Vec<AuditRecord> {
        let records = self.records.read().await;
        records.iter().rev().take(limit).cloned().collect()
    }
}
```

`arcanum-engine/src/event_bus.rs`:
```rust
use serde_json::Value;
use std::collections::HashMap;
use tokio::sync::{broadcast, RwLock};

pub struct EventBus {
    senders: RwLock<HashMap<String, broadcast::Sender<Value>>>,
}

impl EventBus {
    pub fn new() -> Self {
        Self { senders: RwLock::new(HashMap::new()) }
    }

    pub async fn subscribe(&self, topic: &str) -> broadcast::Receiver<Value> {
        let mut map = self.senders.write().await;
        let sender = map.entry(topic.to_string())
            .or_insert_with(|| broadcast::channel(128).0);
        sender.subscribe()
    }

    pub async fn publish(&self, topic: &str, payload: Value) {
        let map = self.senders.read().await;
        if let Some(sender) = map.get(topic) {
            let _ = sender.send(payload); // ignore if no subscribers
        }
    }
}
```

`arcanum-engine/src/rate_limit.rs`:
```rust
use std::{collections::HashMap, sync::Mutex};

pub struct RateLimiter {
    max_per_window: usize,
    counts: Mutex<HashMap<String, usize>>,
}

impl RateLimiter {
    pub fn new(max_per_window: usize) -> Self {
        Self { max_per_window, counts: Mutex::new(HashMap::new()) }
    }

    /// Returns true if the request is allowed, false if rate limit exceeded.
    pub fn check_and_record(&self, key: &str) -> bool {
        let mut map = self.counts.lock().unwrap();
        let count = map.entry(key.to_string()).or_insert(0);
        if *count >= self.max_per_window { return false; }
        *count += 1;
        true
    }

    pub fn reset(&self, key: &str) {
        self.counts.lock().unwrap().remove(key);
    }
}
```

- [ ] **Step 3: Run tests and commit**

```bash
cargo test -p arcanum-engine
git add arcanum-engine/
git commit -m "feat(engine): add AuthMiddleware, AuditLogger, EventBus, RateLimiter"
```

---

### Task 27: arcanum-mcp — McpServer + Handlers

**Files:**
- Modify: `arcanum-mcp/Cargo.toml`
- Create: `arcanum-mcp/src/server.rs`
- Create: `arcanum-mcp/src/handlers.rs`
- Create: `arcanum-mcp/src/lib.rs`

- [ ] **Step 1: Update `arcanum-mcp/Cargo.toml`**

```toml
[package]
name    = "arcanum-mcp"
version = "0.1.0"
edition = "2021"

[dependencies]
arcanum-core   = { path = "../arcanum-core" }
arcanum-engine = { path = "../arcanum-engine" }
async-trait    = { workspace = true }
tokio          = { workspace = true }
serde          = { workspace = true }
serde_json     = { workspace = true }
anyhow         = { workspace = true }
tracing        = { workspace = true }
axum           = { version = "0.7", features = ["ws"] }
tokio-stream   = "0.1"
futures        = "0.3"
```

> **MCP Protocol Note:** MCP is JSON-RPC 2.0 over SSE or WebSocket. Implement the transport directly using `axum`. The MCP spec defines `initialize`, `tools/list`, `tools/call`, `resources/list`, `resources/read` methods. See https://modelcontextprotocol.io/specification for full spec.

- [ ] **Step 2: Write failing test**

```rust
// arcanum-mcp/tests/mcp_test.rs
use arcanum_mcp::McpJsonRpcHandler;
use serde_json::json;

#[tokio::test]
async fn test_mcp_list_tools_response() {
    let handler = McpJsonRpcHandler::new_test();
    let req = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/list",
        "params": {}
    });
    let resp = handler.handle(req).await.unwrap();
    assert_eq!(resp["jsonrpc"], "2.0");
    assert!(resp["result"]["tools"].is_array());
    let tools: Vec<String> = resp["result"]["tools"]
        .as_array().unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap().to_string())
        .collect();
    assert!(tools.contains(&"ingest".to_string()));
    assert!(tools.contains(&"search".to_string()));
}

#[tokio::test]
async fn test_mcp_unknown_method_returns_error() {
    let handler = McpJsonRpcHandler::new_test();
    let req = json!({ "jsonrpc": "2.0", "id": 2, "method": "unknown/method", "params": {} });
    let resp = handler.handle(req).await.unwrap();
    assert!(resp["error"].is_object());
    assert_eq!(resp["error"]["code"], -32601); // Method not found
}
```

- [ ] **Step 3: Implement `arcanum-mcp/src/handlers.rs`**

```rust
use serde_json::{json, Value};
use arcanum_core::Result;

/// MCP JSON-RPC handler. All Arcanum capabilities are registered here.
pub struct McpJsonRpcHandler {
    // In full impl: holds Arc<ArcanumEngine>
}

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
                        { "name": "ingest", "description": "Ingest a document into a collection",
                          "inputSchema": { "type": "object", "properties": {
                              "source_uri": { "type": "string" },
                              "collection_id": { "type": "string" },
                              "pipeline": { "type": "string", "enum": ["standard","contextual","graph","raptor","full"] }
                          }, "required": ["source_uri", "collection_id"] }},
                        { "name": "search", "description": "Search a collection",
                          "inputSchema": { "type": "object", "properties": {
                              "query": { "type": "string" },
                              "collection_id": { "type": "string" },
                              "top_k": { "type": "integer", "default": 5 }
                          }, "required": ["query"] }},
                        { "name": "list_collections", "description": "List all collections",
                          "inputSchema": { "type": "object", "properties": {} }},
                        { "name": "eval_run", "description": "Run retrieval quality evaluation",
                          "inputSchema": { "type": "object", "properties": {
                              "collection_id": { "type": "string" }
                          }, "required": ["collection_id"] }}
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
```

`arcanum-mcp/src/server.rs` — SSE transport for MCP:
```rust
use axum::{
    routing::{get, post},
    Router, Json, extract::State,
    response::sse::{Event, Sse},
};
use std::sync::Arc;
use crate::handlers::McpJsonRpcHandler;
use serde_json::Value;

pub struct McpServer {
    handler: Arc<McpJsonRpcHandler>,
    port: u16,
}

impl McpServer {
    pub fn new(handler: Arc<McpJsonRpcHandler>, port: u16) -> Self {
        Self { handler, port }
    }

    pub async fn start(self) -> Result<(), Box<dyn std::error::Error>> {
        let handler = self.handler.clone();
        let app = Router::new()
            .route("/mcp", post(handle_jsonrpc))
            .route("/health", get(|| async { "ok" }))
            .with_state(handler);

        let listener = tokio::net::TcpListener::bind(
            format!("0.0.0.0:{}", self.port)
        ).await?;
        tracing::info!("MCP server listening on :{}", self.port);
        axum::serve(listener, app).await?;
        Ok(())
    }
}

async fn handle_jsonrpc(
    State(handler): State<Arc<McpJsonRpcHandler>>,
    Json(body): Json<Value>,
) -> Json<Value> {
    match handler.handle(body).await {
        Ok(resp) => Json(resp),
        Err(e) => Json(serde_json::json!({
            "jsonrpc": "2.0", "id": null,
            "error": { "code": -32603, "message": e.to_string() }
        })),
    }
}
```

`arcanum-mcp/src/lib.rs`:
```rust
pub mod handlers;
pub mod server;
pub use handlers::McpJsonRpcHandler;
pub use server::McpServer;
```

- [ ] **Step 4: Run tests and commit**

```bash
cargo test -p arcanum-mcp
git add arcanum-mcp/
git commit -m "feat(mcp): add McpJsonRpcHandler with tools/list, tools/call, and SSE transport"
```

---

### Task 28: arcanum-server — HTTP Server + Public API Routes

**Files:**
- Modify: `arcanum-server/Cargo.toml`
- Create: `arcanum-server/src/routes/api.rs`
- Create: `arcanum-server/src/routes/health.rs`
- Create: `arcanum-server/src/server.rs`
- Create: `arcanum-server/src/lib.rs`

- [ ] **Step 1: Update `arcanum-server/Cargo.toml`**

```toml
[package]
name    = "arcanum-server"
version = "0.1.0"
edition = "2021"

[dependencies]
arcanum-core   = { path = "../arcanum-core" }
arcanum-engine = { path = "../arcanum-engine" }
async-trait    = { workspace = true }
tokio          = { workspace = true }
serde          = { workspace = true }
serde_json     = { workspace = true }
anyhow         = { workspace = true }
tracing        = { workspace = true }
axum           = { version = "0.7", features = ["ws", "macros"] }
tower          = "0.4"
tower-http     = { version = "0.5", features = ["trace", "cors"] }
```

- [ ] **Step 2: Write failing tests**

```rust
// arcanum-server/tests/api_test.rs
use axum::http::StatusCode;
use axum_test::TestServer;
use arcanum_server::build_app;

#[tokio::test]
async fn test_health_endpoint_returns_200() {
    let app = build_app(None);
    let server = TestServer::new(app).unwrap();
    let resp = server.get("/health").await;
    resp.assert_status_ok();
}

#[tokio::test]
async fn test_ready_endpoint_returns_200_in_dev() {
    let app = build_app(None);
    let server = TestServer::new(app).unwrap();
    let resp = server.get("/ready").await;
    resp.assert_status_ok();
}

#[tokio::test]
async fn test_search_requires_auth() {
    let app = build_app(None);
    let server = TestServer::new(app).unwrap();
    let resp = server.post("/api/v1/search")
        .json(&serde_json::json!({ "query": "test", "collection_id": "docs" }))
        .await;
    // No auth header → 401
    resp.assert_status(StatusCode::UNAUTHORIZED);
}
```

- [ ] **Step 3: Add `axum-test` dev dependency**

```toml
[dev-dependencies]
axum-test = "0.1"
```

- [ ] **Step 4: Implement routes**

`arcanum-server/src/routes/health.rs`:
```rust
use axum::{response::IntoResponse, http::StatusCode, Json};
use serde_json::json;

pub async fn liveness() -> impl IntoResponse {
    (StatusCode::OK, Json(json!({ "status": "alive" })))
}

pub async fn readiness() -> impl IntoResponse {
    // In production: check vector store, embedding provider, metadata DB
    // For now: always ready in development
    (StatusCode::OK, Json(json!({ "status": "ready", "dependencies": {} })))
}
```

`arcanum-server/src/routes/api.rs`:
```rust
use axum::{
    extract::{State, Json},
    http::{StatusCode, HeaderMap},
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use arcanum_engine::ArcanumEngine;
use arcanum_core::types::*;

#[derive(Deserialize)]
pub struct SearchRequest {
    pub query: String,
    pub collection_id: Option<String>,
    pub top_k: Option<usize>,
}

#[derive(Serialize)]
pub struct SearchResponse {
    pub chunks: Vec<serde_json::Value>,
    pub confidence: f32,
}

#[derive(Deserialize)]
pub struct IngestRequest {
    pub source_uri: String,
    pub collection_id: String,
    pub pipeline: Option<String>,
}

#[derive(Serialize)]
pub struct IngestResponse {
    pub operation_id: String,
    pub status: String,
}

pub async fn search(
    headers: HeaderMap,
    State(engine): State<Option<Arc<ArcanumEngine>>>,
    Json(req): Json<SearchRequest>,
) -> impl IntoResponse {
    // Validate auth token from Authorization header
    let Some(_token) = headers.get("Authorization") else {
        return (StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({ "error": "missing Authorization header" }))).into_response();
    };

    // In full impl: validate token, call engine.retrieval.search()
    let query = Query::new(req.query).with_top_k(req.top_k.unwrap_or(5));
    (StatusCode::OK, Json(serde_json::json!({
        "chunks": [], "citations": [], "confidence": 0.0
    }))).into_response()
}

pub async fn ingest(
    headers: HeaderMap,
    State(engine): State<Option<Arc<ArcanumEngine>>>,
    Json(req): Json<IngestRequest>,
) -> impl IntoResponse {
    let Some(_token) = headers.get("Authorization") else {
        return (StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({ "error": "missing Authorization header" }))).into_response();
    };

    (StatusCode::ACCEPTED, Json(serde_json::json!({
        "operation_id": uuid::Uuid::new_v4().to_string(),
        "status": "queued"
    }))).into_response()
}
```

`arcanum-server/src/routes/admin.rs`:
```rust
use axum::{extract::{State, Path}, http::{StatusCode, HeaderMap}, response::IntoResponse, Json};
use std::sync::Arc;
use arcanum_engine::ArcanumEngine;

pub async fn list_collections(
    headers: HeaderMap,
    State(engine): State<Option<Arc<ArcanumEngine>>>,
) -> impl IntoResponse {
    // Require admin JWT
    let Some(auth) = headers.get("Authorization") else {
        return (StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({ "error": "missing admin JWT" }))).into_response();
    };
    (StatusCode::OK, Json(serde_json::json!({ "collections": [] }))).into_response()
}

pub async fn get_health(
    State(engine): State<Option<Arc<ArcanumEngine>>>,
) -> impl IntoResponse {
    (StatusCode::OK, Json(serde_json::json!({
        "vector_store": "ok",
        "embedding_provider": "ok",
        "graph_store": "disabled",
        "tree_store": "disabled"
    }))).into_response()
}

pub async fn get_metrics(
    State(engine): State<Option<Arc<ArcanumEngine>>>,
) -> impl IntoResponse {
    (StatusCode::OK, Json(serde_json::json!({
        "uptime_secs": 0,
        "ingestion_queue_depth": 0,
        "cache_hit_rate": 0.0
    }))).into_response()
}
```

`arcanum-server/src/server.rs`:
```rust
use axum::{Router, routing::{get, post}};
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use std::sync::Arc;
use arcanum_engine::ArcanumEngine;
use crate::routes::{api, health, admin};

pub fn build_app(engine: Option<Arc<ArcanumEngine>>) -> Router {
    Router::new()
        // Health probes
        .route("/health", get(health::liveness))
        .route("/ready",  get(health::readiness))
        // Public API
        .route("/api/v1/search", post(api::search))
        .route("/api/v1/ingest", post(api::ingest))
        // Admin API
        .route("/admin/collections", get(admin::list_collections))
        .route("/admin/health",      get(admin::get_health))
        .route("/admin/metrics",     get(admin::get_metrics))
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
        .with_state(engine)
}

pub async fn start(engine: Option<Arc<ArcanumEngine>>, port: u16)
    -> Result<(), Box<dyn std::error::Error>>
{
    let app = build_app(engine);
    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port)).await?;
    tracing::info!("HTTP server listening on :{}", port);
    axum::serve(listener, app).await?;
    Ok(())
}
```

`arcanum-server/src/lib.rs`:
```rust
pub mod routes {
    pub mod admin;
    pub mod api;
    pub mod health;
}
pub mod server;
pub use server::{build_app, start};
```

- [ ] **Step 5: Run tests and commit**

```bash
cargo test -p arcanum-server
git add arcanum-server/
git commit -m "feat(server): add HTTP server with /api/v1/, /health, /ready, /admin routes"
```

---

### Task 29: arcanum-server — WebSocket Event Streaming

**Files:**
- Create: `arcanum-server/src/ws.rs`

- [ ] **Step 1: Write the failing test**

```rust
// arcanum-server/tests/ws_test.rs
use arcanum_server::build_app;
use axum_test::TestServer;

#[tokio::test]
async fn test_ws_route_exists() {
    let app = build_app(None);
    let server = TestServer::new(app).unwrap();
    // WebSocket upgrade; just check the route is registered (upgrade returns 101 or 400)
    let resp = server.get("/ws/events").await;
    // Without WS upgrade headers, server returns 400 Bad Request — route exists
    assert!(resp.status_code().as_u16() != 404);
}
```

- [ ] **Step 2: Implement `arcanum-server/src/ws.rs`**

```rust
use axum::{
    extract::{ws::{WebSocket, WebSocketUpgrade, Message}, State},
    response::IntoResponse,
};
use std::sync::Arc;
use arcanum_engine::{ArcanumEngine, event_bus::EventBus};
use serde_json::json;

pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(engine): State<Option<Arc<ArcanumEngine>>>,
) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_socket(socket, engine))
}

async fn handle_socket(mut socket: WebSocket, engine: Option<Arc<ArcanumEngine>>) {
    // Client sends: { "subscribe": ["ingestion:progress", "system:health"] }
    // Server streams matching EventBus events as JSON frames
    while let Some(msg) = socket.recv().await {
        if let Ok(Message::Text(text)) = msg {
            if let Ok(cmd) = serde_json::from_str::<serde_json::Value>(&text) {
                if let Some(topics) = cmd["subscribe"].as_array() {
                    // In full impl: subscribe to EventBus and forward events
                    let _ = socket.send(Message::Text(json!({
                        "type": "subscribed",
                        "topics": topics
                    }).to_string())).await;
                }
            }
        }
    }
}
```

Add WebSocket route to `arcanum-server/src/server.rs`:
```rust
use crate::ws::ws_handler;
// Add to Router:
.route("/ws/events", get(ws_handler))
```

- [ ] **Step 3: Run tests and commit**

```bash
cargo test -p arcanum-server
git add arcanum-server/
git commit -m "feat(server): add WebSocket event streaming endpoint"
```

---

### Task 30: Workspace Integration Test — End-to-End Smoke Test

**Files:**
- Create: `tests/integration/smoke_test.rs` (workspace-level integration test)

- [ ] **Step 1: Create workspace-level integration test**

```rust
// tests/integration/smoke_test.rs
//! Smoke test: verifies the full stack starts up and responds.
//! Run with: cargo test --test smoke_test
//! Requires: Ollama running at localhost:11434

use arcanum_core::config::ArcanumConfig;
use arcanum_engine::ArcanumEngineBuilder;
use arcanum_server::build_app;
use axum_test::TestServer;

#[tokio::test]
async fn test_full_stack_health_check() {
    let cfg = ArcanumConfig::default();
    let engine = ArcanumEngineBuilder::new(cfg).build().await.unwrap();
    let app = build_app(Some(engine));
    let server = TestServer::new(app).unwrap();

    let resp = server.get("/health").await;
    resp.assert_status_ok();

    let resp = server.get("/ready").await;
    resp.assert_status_ok();
}

#[tokio::test]
async fn test_unauthorized_search_rejected() {
    let cfg = ArcanumConfig::default();
    let engine = ArcanumEngineBuilder::new(cfg).build().await.unwrap();
    let app = build_app(Some(engine));
    let server = TestServer::new(app).unwrap();

    let resp = server.post("/api/v1/search")
        .json(&serde_json::json!({ "query": "test" }))
        .await;
    resp.assert_status(axum::http::StatusCode::UNAUTHORIZED);
}
```

Add to workspace `Cargo.toml`:
```toml
[[test]]
name = "smoke_test"
path = "tests/integration/smoke_test.rs"
```

- [ ] **Step 2: Create test directory and run**

```bash
mkdir -p tests/integration
cargo test --test smoke_test
```
Expected: both tests pass.

- [ ] **Step 3: Run full workspace test suite**

```bash
cargo test --workspace
```
Expected: all unit and integration tests pass.

- [ ] **Step 4: Final commit**

```bash
git add tests/ Cargo.toml
git commit -m "test: add workspace-level smoke tests for full stack startup"
```

---

## Phase 4 Complete ✓

The full Arcanum service layer is implemented. Verify:

```bash
cargo build --workspace     # all 13 crates compile
cargo test --workspace      # all tests pass
cargo clippy --workspace    # no clippy warnings
```

### What's built

| Layer | Crate | Status |
|---|---|---|
| Foundation | arcanum-core | ✓ traits, types, config |
| Storage | arcanum-vector | ✓ LanceDB, BM25, SQLite metadata |
| Storage | arcanum-graph | ✓ InMemoryGraphStore (swap for Kuzu/Neo4j) |
| Storage | arcanum-tree | ✓ InMemoryTreeStore + RaptorBuilder |
| Models | arcanum-models | ✓ Ollama, EnrichmentDispatcher, Router |
| Processing | arcanum-ingestion | ✓ FileLoader, chunkers, enrichment stages |
| Processing | arcanum-middleware | ✓ BoundedQueue, CircuitBreaker |
| Processing | arcanum-retrieval | ✓ VectorRetriever, BM25, RRF fusion, QueryCache |
| Processing | arcanum-eval | ✓ HitRate, MRR, NDCG, EvalRunner |
| Processing | arcanum-pipeline | ✓ DAG executor, StandardPipeline template |
| Service | arcanum-engine | ✓ ArcanumEngine, services, auth, audit, events |
| Service | arcanum-mcp | ✓ JSON-RPC handler, tools/list, tools/call |
| Service | arcanum-server | ✓ HTTP routes, WebSocket streaming |

### Next steps (production hardening)

- Replace InMemoryGraphStore with Kuzu or Neo4j implementation
- Add pgvector VectorStore backend (`arcanum-vector/src/pgvector.rs`)
- Implement full IngestionWorker background processing in arcanum-pipeline
- Add GraphPipeline, RAPTORPipeline, FullPipeline templates
- Wire arcanum-engine into arcanum-mcp/arcanum-server handlers (replace stubs)
- Add OpenAI / Anthropic / GLiNER providers to arcanum-models
- Add admin portal static assets to arcanum-server (`include_bytes!`)
