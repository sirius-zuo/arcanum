use super::{require_key, DemoCtx, DemoError};
use async_trait::async_trait;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::Json;
use serde::Serialize;
use std::time::Duration;

const EMBED_MODEL: &str = "nomic-embed-text";

/// Lists the model names an Ollama server has pulled (`GET /api/tags`).
#[async_trait]
pub trait OllamaProbe: Send + Sync {
    async fn tags(&self) -> Result<Vec<String>, String>;
}

pub struct HttpOllamaProbe {
    base_url: String,
    client: reqwest::Client,
}

impl HttpOllamaProbe {
    pub fn new(base_url: &str) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(2))
            .build()
            .unwrap_or_default();
        HttpOllamaProbe {
            base_url: base_url.trim_end_matches('/').to_string(),
            client,
        }
    }
}

#[async_trait]
impl OllamaProbe for HttpOllamaProbe {
    async fn tags(&self) -> Result<Vec<String>, String> {
        let resp = self
            .client
            .get(format!("{}/api/tags", self.base_url))
            .send()
            .await
            .map_err(|e| e.to_string())?
            .error_for_status()
            .map_err(|e| e.to_string())?;
        let body: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
        Ok(body["models"]
            .as_array()
            .map(|models| {
                models
                    .iter()
                    .filter_map(|m| m["name"].as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default())
    }
}

#[derive(Serialize)]
pub struct Check {
    pub id: &'static str,
    pub label: &'static str,
    pub ok: bool,
    pub detail: String,
    pub fix: Option<String>,
}

#[derive(Serialize)]
pub struct Health {
    pub ready: bool,
    pub checks: Vec<Check>,
}

fn check(
    id: &'static str,
    label: &'static str,
    ok: bool,
    detail: impl Into<String>,
    fix: impl Into<String>,
) -> Check {
    Check {
        id,
        label,
        ok,
        detail: detail.into(),
        fix: if ok { None } else { Some(fix.into()) },
    }
}

fn engine_check(id: &'static str, label: &'static str, present: bool) -> Check {
    check(
        id,
        label,
        present,
        if present { "Configured" } else { "Missing" },
        "Rebuild the example: this service is wired in src/engine_setup.rs",
    )
}

pub async fn health(
    State(ctx): State<DemoCtx>,
    headers: HeaderMap,
) -> Result<Json<Health>, DemoError> {
    require_key(&ctx.state, &headers)?;
    let s = &ctx.state;
    let e = &s.engine;
    let anthropic = s.settings.anthropic_key.is_some();
    let chat = &s.settings.chat_model;
    let tags = ctx.probe.tags().await;

    let mut checks = Vec::new();
    let start = "Start Ollama: ollama serve";
    match &tags {
        Ok(t) => checks.push(check(
            "ollama",
            "Ollama reachable",
            true,
            format!("{} models available at {}", t.len(), s.settings.ollama_url),
            "",
        )),
        Err(err) => checks.push(check(
            "ollama",
            "Ollama reachable",
            false,
            format!("{}: {err}", s.settings.ollama_url),
            start,
        )),
    }
    let embed_ok = tags
        .as_ref()
        .is_ok_and(|t| t.iter().any(|n| n.starts_with(EMBED_MODEL)));
    checks.push(check(
        "embed_model",
        "Embedding model",
        embed_ok,
        if embed_ok {
            format!("{EMBED_MODEL} is pulled")
        } else {
            format!("{EMBED_MODEL} not found")
        },
        if tags.is_err() {
            start.to_string()
        } else {
            format!("ollama pull {EMBED_MODEL}")
        },
    ));
    let chat_ok = anthropic
        || tags
            .as_ref()
            .is_ok_and(|t| t.iter().any(|n| n == chat || n.starts_with(chat.as_str())));
    checks.push(check(
        "chat_model",
        "Chat model",
        chat_ok,
        match (chat_ok, anthropic) {
            (true, true) => "Claude is enabled".to_string(),
            (true, false) => format!("{chat} is pulled"),
            _ => format!("{chat} not found"),
        },
        if tags.is_err() {
            start.to_string()
        } else {
            format!("ollama pull {chat}")
        },
    ));
    checks.push(engine_check(
        "vector_store",
        "Vector store",
        e.vector_store.is_some(),
    ));
    checks.push(engine_check(
        "lexical_index",
        "Lexical index",
        e.bm25_index.is_some(),
    ));
    checks.push(engine_check(
        "graph_store",
        "Graph store",
        e.graph_store.is_some(),
    ));
    checks.push(engine_check(
        "tree_store",
        "Tree store",
        e.tree_store.is_some(),
    ));
    checks.push(engine_check(
        "registry",
        "Chunk registry",
        e.chunk_metadata_store.is_some(),
    ));
    checks.push(engine_check(
        "generate",
        "Generate service",
        e.generate.is_some(),
    ));
    checks.push(engine_check("verify", "Verify service", e.verify.is_some()));
    checks.push(engine_check(
        "evidence",
        "Evidence resolver",
        e.evidence.is_some(),
    ));

    let ok = |id: &str| checks.iter().any(|c| c.id == id && c.ok);
    let ready = [
        "ollama",
        "embed_model",
        "chat_model",
        "vector_store",
        "registry",
        "generate",
        "verify",
    ]
    .iter()
    .all(|id| ok(id));
    Ok(Json(Health { ready, checks }))
}
