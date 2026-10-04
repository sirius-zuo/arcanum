//! Streaming generator for Ollama's OpenAI-compatible endpoint that switches reasoning off.
//!
//! Reasoning models (the qwen3 family, for example) stream their thinking in a separate
//! `reasoning` field and spend the whole token budget on it, so `content` stays empty and the
//! engine reports a timeout or an unparsable judge reply. Ollama honors `reasoning_effort:
//! "none"` on `/v1/chat/completions`; models without a thinking mode ignore it.
//! `arcanum_models::OpenAiCompatibleGenerator` cannot send that field, hence this small copy.

use arcanum_core::traits::{
    GenerationEvent, GenerationRequest, GenerationUsage, Generator, StopReason,
};
use arcanum_core::{ArcanumError, Result};
use arcanum_models::sse::SseParser;
use async_trait::async_trait;
use futures::stream::BoxStream;
use futures::StreamExt;
use serde_json::{json, Value};
use std::collections::VecDeque;

pub struct OllamaGenerator {
    model: String,
    base_url: String,
    client: reqwest::Client,
}

impl OllamaGenerator {
    /// `base_url` is the OpenAI-compatible root, for example `http://localhost:11434/v1`.
    pub fn new(model: impl Into<String>, base_url: impl Into<String>) -> Self {
        Self {
            model: model.into(),
            base_url: base_url.into().trim_end_matches('/').to_string(),
            client: reqwest::Client::new(),
        }
    }
}

fn request_body(model: &str, req: &GenerationRequest) -> Value {
    let mut messages = vec![json!({"role": "system", "content": req.system})];
    messages.extend(req.messages.iter().map(|m| json!(m)));
    let mut body = json!({
        "model": model,
        "messages": messages,
        "max_tokens": req.max_tokens,
        "stream": true,
        "stream_options": {"include_usage": true},
        "reasoning_effort": "none",
    });
    if let Some(t) = req.temperature {
        body["temperature"] = json!(t);
    }
    body
}

fn map_stop(reason: Option<&str>) -> StopReason {
    match reason {
        Some("stop") => StopReason::EndTurn,
        Some("length") => StopReason::MaxTokens,
        Some(other) => StopReason::Other(other.to_string()),
        None => StopReason::Other("unknown".to_string()),
    }
}

struct State {
    bytes: BoxStream<'static, reqwest::Result<bytes::Bytes>>,
    parser: SseParser,
    queue: VecDeque<arcanum_models::sse::SseEvent>,
    usage: GenerationUsage,
    finish: Option<String>,
    finished: bool,
}

async fn next_event(mut st: State) -> Option<(Result<GenerationEvent>, State)> {
    if st.finished {
        return None;
    }
    loop {
        let ev = match st.queue.pop_front() {
            Some(ev) => ev,
            None => match st.bytes.next().await {
                Some(Ok(chunk)) => {
                    st.queue.extend(st.parser.push(&chunk));
                    continue;
                }
                Some(Err(e)) => {
                    st.finished = true;
                    return Some((Err(ArcanumError::Generation(e.to_string())), st));
                }
                None => {
                    st.finished = true;
                    let err = ArcanumError::Generation("stream ended before completion".into());
                    return Some((Err(err), st));
                }
            },
        };
        if ev.data.trim() == "[DONE]" {
            st.finished = true;
            let done = GenerationEvent::Done {
                usage: st.usage.clone(),
                stop_reason: map_stop(st.finish.take().as_deref()),
            };
            return Some((Ok(done), st));
        }
        let v: Value = serde_json::from_str(&ev.data).unwrap_or(Value::Null);
        if v["error"].is_object() {
            st.finished = true;
            let msg = v["error"]["message"].as_str().unwrap_or("unknown error");
            return Some((Err(ArcanumError::Generation(msg.to_string())), st));
        }
        if let Some(r) = v["choices"][0]["finish_reason"].as_str() {
            st.finish = Some(r.to_string());
        }
        if let Some(n) = v["usage"]["prompt_tokens"].as_u64() {
            st.usage.input_tokens = Some(n as u32);
        }
        if let Some(n) = v["usage"]["completion_tokens"].as_u64() {
            st.usage.output_tokens = Some(n as u32);
        }
        if let Some(t) = v["choices"][0]["delta"]["content"].as_str() {
            if !t.is_empty() {
                return Some((Ok(GenerationEvent::TextDelta(t.to_string())), st));
            }
        }
    }
}

#[async_trait]
impl Generator for OllamaGenerator {
    async fn stream(
        &self,
        req: GenerationRequest,
    ) -> Result<BoxStream<'static, Result<GenerationEvent>>> {
        let resp = self
            .client
            .post(format!("{}/chat/completions", self.base_url))
            .json(&request_body(&self.model, &req))
            .send()
            .await
            .map_err(|e| ArcanumError::Generation(e.to_string()))?;
        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(ArcanumError::Generation(format!(
                "ollama returned {status}: {text}"
            )));
        }
        let state = State {
            bytes: resp.bytes_stream().boxed(),
            parser: SseParser::new(),
            queue: VecDeque::new(),
            usage: GenerationUsage::default(),
            finish: None,
            finished: false,
        };
        Ok(futures::stream::unfold(state, next_event).boxed())
    }

    fn model(&self) -> &str {
        &self.model
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arcanum_core::types::{Message, Role};
    use axum::{extract::State as AxState, routing::post, Json, Router};
    use std::sync::{Arc, Mutex};

    #[tokio::test]
    async fn sends_reasoning_off_and_streams_content_only() {
        let seen: Arc<Mutex<Option<Value>>> = Arc::default();
        let sse = concat!(
            "data: {\"choices\":[{\"delta\":{\"reasoning\":\"thinking\"}}]}\n\n",
            "data: {\"choices\":[{\"delta\":{\"content\":\"Hi\"},\"finish_reason\":\"stop\"}]}\n\n",
            "data: {\"choices\":[],\"usage\":{\"prompt_tokens\":3,\"completion_tokens\":1}}\n\n",
            "data: [DONE]\n\n",
        );
        let app = Router::new()
            .route(
                "/v1/chat/completions",
                post(
                    move |AxState(seen): AxState<Arc<Mutex<Option<Value>>>>,
                          Json(b): Json<Value>| async move {
                        *seen.lock().unwrap() = Some(b);
                        ([("content-type", "text/event-stream")], sse)
                    },
                ),
            )
            .with_state(seen.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

        let g = OllamaGenerator::new("m", format!("http://{addr}/v1/"));
        let req = GenerationRequest {
            system: "sys".into(),
            messages: vec![Message {
                role: Role::User,
                content: "hi".into(),
            }],
            max_tokens: 64,
            temperature: Some(0.0),
        };
        let events: Vec<_> = g.stream(req).await.unwrap().collect().await;
        let events: Vec<_> = events.into_iter().map(|e| e.unwrap()).collect();
        assert_eq!(
            events,
            vec![
                GenerationEvent::TextDelta("Hi".into()),
                GenerationEvent::Done {
                    usage: GenerationUsage {
                        input_tokens: Some(3),
                        output_tokens: Some(1)
                    },
                    stop_reason: StopReason::EndTurn,
                },
            ]
        );
        let body = seen.lock().unwrap().clone().unwrap();
        assert_eq!(body["reasoning_effort"], "none");
        assert_eq!(body["model"], "m");
        assert_eq!(body["stream"], true);
    }
}
