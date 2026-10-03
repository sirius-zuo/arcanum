//! Streaming generator over an OpenAI-compatible chat completions API.

use crate::sse::{sse_events, SseEvent};
use arcanum_core::traits::{
    GenerationEvent, GenerationRequest, GenerationUsage, Generator, StopReason,
};
use arcanum_core::{ArcanumError, Result};
use async_trait::async_trait;
use futures::stream::BoxStream;
use futures::StreamExt;
use serde_json::{json, Value};

pub struct OpenAiCompatibleGenerator {
    model: String,
    api_key: Option<String>,
    base_url: String,
    client: reqwest::Client,
}

impl OpenAiCompatibleGenerator {
    pub fn new(
        model: impl Into<String>,
        api_key: Option<String>,
        base_url: Option<String>,
    ) -> Self {
        Self {
            model: model.into(),
            api_key,
            base_url: base_url
                .unwrap_or_else(|| "https://api.openai.com/v1".to_string())
                .trim_end_matches('/')
                .to_string(),
            client: reqwest::Client::new(),
        }
    }
}

struct State {
    events: BoxStream<'static, Result<SseEvent>>,
    usage: GenerationUsage,
    finish_reason: Option<String>,
    finished: bool,
}

fn map_stop(reason: Option<String>) -> StopReason {
    match reason.as_deref() {
        Some("stop") => StopReason::EndTurn,
        Some("length") => StopReason::MaxTokens,
        Some(other) => StopReason::Other(other.to_string()),
        None => StopReason::Other("unknown".to_string()),
    }
}
#[async_trait]
impl Generator for OpenAiCompatibleGenerator {
    async fn stream(
        &self,
        req: GenerationRequest,
    ) -> Result<BoxStream<'static, Result<GenerationEvent>>> {
        let mut messages = vec![json!({"role": "system", "content": req.system})];
        messages.extend(req.messages.iter().map(|m| json!(m)));
        let mut body = json!({
            "model": self.model,
            "messages": messages,
            "max_tokens": req.max_tokens,
            "stream": true,
            "stream_options": {"include_usage": true},
        });
        if let Some(t) = req.temperature {
            body["temperature"] = json!(t);
        }
        let mut builder = self
            .client
            .post(format!("{}/chat/completions", self.base_url))
            .json(&body);
        if let Some(key) = &self.api_key {
            builder = builder.bearer_auth(key);
        }
        let resp = builder
            .send()
            .await
            .map_err(|e| ArcanumError::Generation(e.to_string()))?;
        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(ArcanumError::Generation(format!(
                "openai_compatible returned {status}: {text}"
            )));
        }
        let state = State {
            events: sse_events(resp),
            usage: GenerationUsage::default(),
            finish_reason: None,
            finished: false,
        };
        Ok(futures::stream::unfold(state, |mut st| async move {
            if st.finished {
                return None;
            }
            loop {
                let ev = match st.events.next().await {
                    Some(Ok(ev)) => ev,
                    Some(Err(e)) => {
                        st.finished = true;
                        return Some((Err(e), st));
                    }
                    None => {
                        st.finished = true;
                        return Some((
                            Err(ArcanumError::Generation(
                                "stream ended before completion".into(),
                            )),
                            st,
                        ));
                    }
                };
                if ev.data.trim() == "[DONE]" {
                    st.finished = true;
                    let done = GenerationEvent::Done {
                        usage: st.usage.clone(),
                        stop_reason: map_stop(st.finish_reason.take()),
                    };
                    return Some((Ok(done), st));
                }
                let v: Value = serde_json::from_str(&ev.data).unwrap_or(Value::Null);
                if v["error"].is_object() {
                    st.finished = true;
                    let msg = v["error"]["message"]
                        .as_str()
                        .unwrap_or("unknown error")
                        .to_string();
                    return Some((Err(ArcanumError::Generation(msg)), st));
                }
                if let Some(r) = v["choices"][0]["finish_reason"].as_str() {
                    st.finish_reason = Some(r.to_string());
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
        })
        .boxed())
    }

    fn model(&self) -> &str {
        &self.model
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt;
    use mockito::Matcher;

    fn req() -> GenerationRequest {
        GenerationRequest {
            system: "sys".into(),
            messages: vec![arcanum_core::types::Message {
                role: arcanum_core::types::Role::User,
                content: "hi".into(),
            }],
            max_tokens: 64,
            temperature: None,
        }
    }

    fn data(v: Value) -> String {
        format!("data: {v}\n\n")
    }

    fn chunk(text: &str, finish: Option<&str>) -> String {
        data(json!({"choices":[{"delta":{"content":text},"finish_reason":finish}]}))
    }

    fn done() -> String {
        "data: [DONE]\n\n".to_string()
    }

    async fn collect(
        server: &mockito::ServerGuard,
        key: Option<&str>,
    ) -> Vec<Result<GenerationEvent>> {
        let g = OpenAiCompatibleGenerator::new("m", key.map(String::from), Some(server.url()));
        g.stream(req()).await.unwrap().collect().await
    }

    #[tokio::test]
    async fn openai_streams_text_and_usage() {
        let mut server = mockito::Server::new_async().await;
        let body = chunk("Hel", None)
            + &chunk("lo", Some("stop"))
            + &data(json!({"choices":[],"usage":{"prompt_tokens":9,"completion_tokens":2}}))
            + &done();
        let m = server
            .mock("POST", "/chat/completions")
            .match_header("authorization", "Bearer k")
            .match_body(Matcher::PartialJson(json!({
                "stream": true,
                "stream_options": {"include_usage": true},
                "max_tokens": 64,
                "messages": [
                    {"role": "system", "content": "sys"},
                    {"role": "user", "content": "hi"}
                ]
            })))
            .with_header("content-type", "text/event-stream")
            .with_body(body)
            .create_async()
            .await;
        let got: Vec<_> = collect(&server, Some("k"))
            .await
            .into_iter()
            .map(|r| r.unwrap())
            .collect();
        m.assert_async().await;
        assert_eq!(
            got,
            vec![
                GenerationEvent::TextDelta("Hel".into()),
                GenerationEvent::TextDelta("lo".into()),
                GenerationEvent::Done {
                    usage: GenerationUsage {
                        input_tokens: Some(9),
                        output_tokens: Some(2)
                    },
                    stop_reason: StopReason::EndTurn,
                },
            ]
        );
    }

    #[tokio::test]
    async fn openai_without_usage_reports_null() {
        let mut server = mockito::Server::new_async().await;
        let _m = server
            .mock("POST", "/chat/completions")
            .with_header("content-type", "text/event-stream")
            .with_body(chunk("x", Some("length")) + &done())
            .create_async()
            .await;
        let got = collect(&server, Some("k")).await;
        assert_eq!(
            got.last().unwrap().as_ref().unwrap(),
            &GenerationEvent::Done {
                usage: GenerationUsage {
                    input_tokens: None,
                    output_tokens: None
                },
                stop_reason: StopReason::MaxTokens,
            }
        );
    }

    #[tokio::test]
    async fn openai_omits_authorization_without_key() {
        let mut server = mockito::Server::new_async().await;
        let m = server
            .mock("POST", "/chat/completions")
            .match_header("authorization", Matcher::Missing)
            .with_header("content-type", "text/event-stream")
            .with_body(chunk("x", Some("stop")) + &done())
            .create_async()
            .await;
        let got = collect(&server, None).await;
        m.assert_async().await;
        assert!(got.iter().all(|r| r.is_ok()));
    }

    #[tokio::test]
    async fn openai_error_object_mid_stream() {
        let mut server = mockito::Server::new_async().await;
        let body = chunk("Hi", None)
            + &data(json!({"error":{"message":"overloaded"}}))
            + &chunk("never", None);
        let _m = server
            .mock("POST", "/chat/completions")
            .with_header("content-type", "text/event-stream")
            .with_body(body)
            .create_async()
            .await;
        let got = collect(&server, Some("k")).await;
        assert_eq!(got.len(), 2);
        assert!(got[1]
            .as_ref()
            .unwrap_err()
            .to_string()
            .contains("overloaded"));
    }

    #[tokio::test]
    async fn openai_stream_without_done_is_error() {
        let mut server = mockito::Server::new_async().await;
        let _m = server
            .mock("POST", "/chat/completions")
            .with_header("content-type", "text/event-stream")
            .with_body(chunk("Hi", None))
            .create_async()
            .await;
        let got = collect(&server, Some("k")).await;
        assert!(got
            .last()
            .unwrap()
            .as_ref()
            .unwrap_err()
            .to_string()
            .contains("stream ended before completion"));
    }

    #[tokio::test]
    async fn openai_non_2xx_is_error() {
        let mut server = mockito::Server::new_async().await;
        let _m = server
            .mock("POST", "/chat/completions")
            .with_status(500)
            .with_body("boom")
            .create_async()
            .await;
        let g = OpenAiCompatibleGenerator::new("m", Some("k".into()), Some(server.url()));
        let err = g.stream(req()).await.err().expect("should be Err");
        assert!(err.to_string().contains("500"), "{err}");
    }
}
