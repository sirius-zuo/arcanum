//! A loopback pass-through to Ollama that switches thinking off for `/api/generate`.
//!
//! The enrichment client in `arcanum_models::OllamaProvider` posts to `/api/generate` and cannot
//! send `think: false`. Reasoning models (the qwen3 family) then think for minutes per call, and
//! Ollama answers concurrent calls with 500, which makes the `full` ingestion pipeline unusable.
//! Atlas points the enricher at this shim instead of at Ollama; every other path and every body
//! is forwarded untouched. Models without a thinking mode ignore the field.
//!
//! It also makes entity extraction work. The framework's extraction prompt names no JSON keys, so
//! a model invents its own (`entity`, `type`) or wraps the JSON in a code fence, the framework's
//! `serde_json::from_str(..).unwrap_or_default()` then yields nothing, and the graph stays empty
//! with no error. The shim swaps that prompt for one that spells out the exact schema and asks
//! Ollama for JSON mode.

use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, Method, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use axum::Router;
use serde_json::Value;

#[derive(Clone)]
struct Shim {
    upstream: String,
    client: reqwest::Client,
}

/// Start of the framework's entity extraction prompt (`build_prompt_for_enricher`).
const EXTRACTION_PREFIX: &str =
    "Extract named entities and relationships from the following text as JSON";
/// What precedes the chunk text in that prompt.
const EXTRACTION_TEXT_MARKER: &str = "\"relations\": [...]}: \n";

/// For the extraction prompt, a replacement that states the exact schema the framework parses
/// (`entities[].name/entity_type`, `relations[].source/relation/target`). `None` for any other prompt.
fn rewrite_extraction_prompt(prompt: &str) -> Option<String> {
    if !prompt.starts_with(EXTRACTION_PREFIX) {
        return None;
    }
    let text = prompt.split_once(EXTRACTION_TEXT_MARKER)?.1;
    Some(format!(
        "Extract the named entities and the relations between them from the text below. \
Reply with only a JSON object of exactly this shape: \
{{\"entities\": [{{\"name\": \"...\", \"entity_type\": \"Person, Organization, Team, Product, System, Place or Other\"}}], \
\"relations\": [{{\"source\": \"<entity name>\", \"relation\": \"<short verb phrase such as leads or owns>\", \"target\": \"<entity name>\"}}]}}. \
Write entity names exactly as they appear in the text, and use only names listed in entities as source and target.\n\nText:\n{text}"
    ))
}

/// Adds `"think": false` to a JSON object body that does not set `think`. Anything else is
/// returned unchanged.
fn think_off(body: &[u8]) -> Vec<u8> {
    match serde_json::from_slice::<Value>(body) {
        Ok(Value::Object(mut o)) => {
            o.entry("think").or_insert(Value::Bool(false));
            let rewritten = o
                .get("prompt")
                .and_then(Value::as_str)
                .and_then(rewrite_extraction_prompt);
            if let Some(prompt) = rewritten {
                o.insert("prompt".into(), Value::String(prompt));
                o.insert("format".into(), Value::String("json".into()));
            }
            serde_json::to_vec(&Value::Object(o)).unwrap_or_else(|_| body.to_vec())
        }
        _ => body.to_vec(),
    }
}

async fn forward(
    State(s): State<Shim>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let body = if uri.path() == "/api/generate" {
        think_off(&body)
    } else {
        body.to_vec()
    };
    let url = format!(
        "{}{}",
        s.upstream,
        uri.path_and_query().map(|p| p.as_str()).unwrap_or("/")
    );
    let mut req = s.client.request(method, url).body(body);
    if let Some(ct) = headers.get("content-type") {
        req = req.header("content-type", ct);
    }
    match req.send().await {
        Ok(resp) => {
            let status =
                StatusCode::from_u16(resp.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
            let ct = resp.headers().get("content-type").cloned();
            match resp.bytes().await {
                Ok(bytes) => {
                    let mut out = (status, bytes).into_response();
                    if let Some(ct) = ct {
                        out.headers_mut().insert("content-type", ct);
                    }
                    out
                }
                Err(e) => (StatusCode::BAD_GATEWAY, e.to_string()).into_response(),
            }
        }
        Err(e) => (StatusCode::BAD_GATEWAY, e.to_string()).into_response(),
    }
}

/// Starts the shim on a free loopback port (needs a running tokio runtime) and returns its base
/// URL, for example `http://127.0.0.1:53124`.
pub fn spawn(upstream: &str) -> std::io::Result<String> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    listener.set_nonblocking(true)?;
    let addr = listener.local_addr()?;
    let app = Router::new().fallback(forward).with_state(Shim {
        upstream: upstream.trim_end_matches('/').to_string(),
        client: reqwest::Client::new(),
    });
    let listener = tokio::net::TcpListener::from_std(listener)?;
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    Ok(format!("http://{addr}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::routing::post;
    use axum::Json;

    #[test]
    fn think_off_sets_false_only_when_absent() {
        let v: Value =
            serde_json::from_slice(&think_off(br#"{"model":"m","prompt":"p"}"#)).unwrap();
        assert_eq!(v["think"], false);
        let v: Value = serde_json::from_slice(&think_off(br#"{"think":true}"#)).unwrap();
        assert_eq!(v["think"], true);
        assert_eq!(think_off(b"not json"), b"not json");
    }

    #[test]
    fn extraction_prompt_is_replaced_with_the_exact_schema_and_json_mode() {
        let original = format!(
            "{EXTRACTION_PREFIX} {{\"entities\": [...], \"relations\": [...]}}: \nMaren Voss leads Halcyon."
        );
        let body = serde_json::json!({"model": "m", "prompt": original}).to_string();
        let v: Value = serde_json::from_slice(&think_off(body.as_bytes())).unwrap();
        let prompt = v["prompt"].as_str().unwrap();
        assert!(prompt.contains("entity_type") && prompt.contains("\"source\""));
        assert!(prompt.ends_with("Maren Voss leads Halcyon."));
        assert_eq!(v["format"], "json");

        let other = serde_json::json!({"model": "m", "prompt": "Summarize the following text concisely:\nx"}).to_string();
        let v: Value = serde_json::from_slice(&think_off(other.as_bytes())).unwrap();
        assert!(v.get("format").is_none());
        assert_eq!(v["prompt"], "Summarize the following text concisely:\nx");
    }

    #[tokio::test]
    async fn generate_gets_think_false_and_other_paths_are_untouched() {
        let app = Router::new()
            .route(
                "/api/generate",
                post(|Json(b): Json<Value>| async move { Json(b) }),
            )
            .route(
                "/api/embeddings",
                post(|Json(b): Json<Value>| async move { Json(b) }),
            );
        let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let up = format!("http://{}", l.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(l, app).await.unwrap() });

        let shim = spawn(&up).unwrap();
        let c = reqwest::Client::new();
        let gen: Value = c
            .post(format!("{shim}/api/generate"))
            .json(&serde_json::json!({"model":"m","prompt":"p","stream":false}))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(gen["think"], false);
        assert_eq!(gen["prompt"], "p");
        let emb: Value = c
            .post(format!("{shim}/api/embeddings"))
            .json(&serde_json::json!({"model":"m","prompt":"p"}))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert!(emb.get("think").is_none());
    }
}
