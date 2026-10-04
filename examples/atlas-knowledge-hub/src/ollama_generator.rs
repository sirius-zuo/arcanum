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

fn request_body(model: &str, req: &GenerationRequest, reasoning_off: bool) -> Value {
    let mut messages = vec![json!({"role": "system", "content": req.system})];
    messages.extend(req.messages.iter().map(|m| json!(m)));
    let mut body = json!({
        "model": model,
        "messages": messages,
        "max_tokens": req.max_tokens,
        "stream": true,
        "stream_options": {"include_usage": true},
    });
    if reasoning_off {
        body["reasoning_effort"] = json!("none");
    }
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
        let url = format!("{}/chat/completions", self.base_url);
        let send = |reasoning_off: bool| {
            self.client
                .post(&url)
                .json(&request_body(&self.model, &req, reasoning_off))
                .send()
        };
        let mut resp = send(true)
            .await
            .map_err(|e| ArcanumError::Generation(e.to_string()))?;
        // An Ollama that does not know `reasoning_effort` answers 400: retry once without it.
        if resp.status() == reqwest::StatusCode::BAD_REQUEST {
            tracing::warn!("ollama rejected reasoning_effort; retrying without it");
            resp = send(false)
                .await
                .map_err(|e| ArcanumError::Generation(e.to_string()))?;
        }
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
    use axum::http::StatusCode;
    use axum::response::{IntoResponse, Response};
    use axum::{routing::post, Json, Router};
    use std::sync::{Arc, Mutex};

    type Seen = Arc<Mutex<Vec<Value>>>;

    /// Serves `/v1/chat/completions` with `handler`, recording every request body.
    async fn serve(
        handler: impl Fn(&Value) -> Response + Clone + Send + Sync + 'static,
    ) -> (String, Seen) {
        let seen: Seen = Arc::default();
        let log = seen.clone();
        let app = Router::new().route(
            "/v1/chat/completions",
            post(move |Json(b): Json<Value>| {
                let out = handler(&b);
                log.lock().unwrap().push(b);
                async move { out }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        (format!("http://{addr}/v1/"), seen)
    }

    fn sse(body: &str) -> Response {
        ([("content-type", "text/event-stream")], body.to_string()).into_response()
    }

    fn req() -> GenerationRequest {
        GenerationRequest {
            system: "sys".into(),
            messages: vec![Message {
                role: Role::User,
                content: "hi".into(),
            }],
            max_tokens: 64,
            temperature: Some(0.0),
        }
    }

    async fn collect(url: String) -> Result<Vec<Result<GenerationEvent>>> {
        let g = OllamaGenerator::new("m", url);
        Ok(g.stream(req()).await?.collect().await)
    }

    const OK_BODY: &str = concat!(
        "data: {\"choices\":[{\"delta\":{\"reasoning\":\"thinking\"}}]}\n\n",
        "data: {\"choices\":[{\"delta\":{\"content\":\"Hi\"},\"finish_reason\":\"stop\"}]}\n\n",
        "data: {\"choices\":[],\"usage\":{\"prompt_tokens\":3,\"completion_tokens\":1}}\n\n",
        "data: [DONE]\n\n",
    );

    #[tokio::test]
    async fn sends_reasoning_off_and_streams_content_only() {
        let (url, seen) = serve(|_| sse(OK_BODY)).await;
        let events: Vec<_> = collect(url)
            .await
            .unwrap()
            .into_iter()
            .map(|e| e.unwrap())
            .collect();
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
        let bodies = seen.lock().unwrap();
        assert_eq!(bodies.len(), 1);
        assert_eq!(bodies[0]["reasoning_effort"], "none");
        assert_eq!(bodies[0]["model"], "m");
        assert_eq!(bodies[0]["stream"], true);
        assert_eq!(bodies[0]["messages"][0]["role"], "system");
    }

    #[tokio::test]
    async fn retries_once_without_reasoning_effort_on_400() {
        let (url, seen) = serve(|b| {
            if b.get("reasoning_effort").is_some() {
                (StatusCode::BAD_REQUEST, "unknown field").into_response()
            } else {
                sse(OK_BODY)
            }
        })
        .await;
        let events = collect(url).await.unwrap();
        assert!(events.iter().all(|e| e.is_ok()));
        let bodies = seen.lock().unwrap();
        assert_eq!(bodies.len(), 2);
        assert!(bodies[0].get("reasoning_effort").is_some());
        assert!(bodies[1].get("reasoning_effort").is_none());
    }

    #[tokio::test]
    async fn a_second_400_is_an_error_and_is_not_retried_again() {
        let (url, seen) = serve(|_| (StatusCode::BAD_REQUEST, "nope").into_response()).await;
        let err = collect(url).await.expect_err("error");
        assert!(err.to_string().contains("400"), "{err}");
        assert_eq!(seen.lock().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn non_2xx_is_an_error_without_retry() {
        let (url, seen) =
            serve(|_| (StatusCode::INTERNAL_SERVER_ERROR, "boom").into_response()).await;
        let err = collect(url).await.expect_err("error");
        let msg = err.to_string();
        assert!(msg.contains("500") && msg.contains("boom"), "{msg}");
        assert_eq!(seen.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn error_event_in_the_stream_is_an_error() {
        let (url, _) =
            serve(|_| sse("data: {\"error\":{\"message\":\"model crashed\"}}\n\n")).await;
        let events = collect(url).await.unwrap();
        assert_eq!(events.len(), 1);
        assert!(events[0]
            .as_ref()
            .unwrap_err()
            .to_string()
            .contains("model crashed"));
    }

    #[tokio::test]
    async fn truncated_stream_is_an_error() {
        let (url, _) =
            serve(|_| sse("data: {\"choices\":[{\"delta\":{\"content\":\"Hel\"}}]}\n\n")).await;
        let events = collect(url).await.unwrap();
        assert_eq!(
            events[0].as_ref().unwrap(),
            &GenerationEvent::TextDelta("Hel".into())
        );
        assert!(events[1]
            .as_ref()
            .unwrap_err()
            .to_string()
            .contains("stream ended before completion"));
        assert_eq!(events.len(), 2);
    }
}
