//! Streaming generator over the Anthropic Messages API.

use crate::sse::{sse_events, SseEvent};
use arcanum_core::traits::{
    GenerationEvent, GenerationRequest, GenerationUsage, Generator, StopReason,
};
use arcanum_core::{ArcanumError, Result};
use async_trait::async_trait;
use futures::stream::BoxStream;
use futures::StreamExt;
use serde_json::{json, Value};

pub struct AnthropicGenerator {
    model: String,
    api_key: String,
    base_url: String,
    client: reqwest::Client,
}

impl AnthropicGenerator {
    pub fn new(
        model: impl Into<String>,
        api_key: impl Into<String>,
        base_url: Option<String>,
    ) -> Self {
        Self {
            model: model.into(),
            api_key: api_key.into(),
            base_url: base_url
                .unwrap_or_else(|| "https://api.anthropic.com".to_string())
                .trim_end_matches('/')
                .to_string(),
            client: reqwest::Client::new(),
        }
    }
}

struct State {
    events: BoxStream<'static, Result<SseEvent>>,
    usage: GenerationUsage,
    stop_reason: Option<String>,
    finished: bool,
}

fn map_stop(reason: Option<String>) -> StopReason {
    match reason.as_deref() {
        Some("end_turn") | None => StopReason::EndTurn,
        Some("max_tokens") => StopReason::MaxTokens,
        Some(other) => StopReason::Other(other.to_string()),
    }
}

#[async_trait]
impl Generator for AnthropicGenerator {
    async fn stream(
        &self,
        req: GenerationRequest,
    ) -> Result<BoxStream<'static, Result<GenerationEvent>>> {
        let mut body = json!({
            "model": self.model,
            "system": req.system,
            "messages": req.messages,
            "max_tokens": req.max_tokens,
            "stream": true,
        });
        if let Some(t) = req.temperature {
            body["temperature"] = json!(t);
        }
        let resp = self
            .client
            .post(format!("{}/v1/messages", self.base_url))
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .json(&body)
            .send()
            .await
            .map_err(|e| ArcanumError::Generation(e.to_string()))?;
        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(ArcanumError::Generation(format!(
                "anthropic returned {status}: {text}"
            )));
        }
        let state = State {
            events: sse_events(resp),
            usage: GenerationUsage::default(),
            stop_reason: None,
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
                let v: Value = serde_json::from_str(&ev.data).unwrap_or(Value::Null);
                let kind = v["type"].as_str().or(ev.event.as_deref()).unwrap_or("");
                match kind {
                    "message_start" => {
                        st.usage.input_tokens = v["message"]["usage"]["input_tokens"]
                            .as_u64()
                            .map(|n| n as u32);
                    }
                    "content_block_delta" if v["delta"]["type"] == "text_delta" => {
                        if let Some(t) = v["delta"]["text"].as_str() {
                            return Some((Ok(GenerationEvent::TextDelta(t.to_string())), st));
                        }
                    }
                    "message_delta" => {
                        if let Some(r) = v["delta"]["stop_reason"].as_str() {
                            st.stop_reason = Some(r.to_string());
                        }
                        st.usage.output_tokens =
                            v["usage"]["output_tokens"].as_u64().map(|n| n as u32);
                    }
                    "message_stop" => {
                        st.finished = true;
                        let done = GenerationEvent::Done {
                            usage: st.usage.clone(),
                            stop_reason: map_stop(st.stop_reason.take()),
                        };
                        return Some((Ok(done), st));
                    }
                    "error" => {
                        st.finished = true;
                        let msg = v["error"]["message"]
                            .as_str()
                            .unwrap_or("unknown error")
                            .to_string();
                        return Some((Err(ArcanumError::Generation(msg)), st));
                    }
                    _ => {}
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

    fn ev(name: &str, data: Value) -> String {
        format!("event: {name}\ndata: {data}\n\n")
    }

    fn delta(text: &str) -> String {
        ev(
            "content_block_delta",
            json!({"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":text}}),
        )
    }

    fn end(reason: &str) -> String {
        ev(
            "message_delta",
            json!({"type":"message_delta","delta":{"stop_reason":reason},"usage":{"output_tokens":5}}),
        ) + &ev("message_stop", json!({"type":"message_stop"}))
    }

    fn start() -> String {
        ev(
            "message_start",
            json!({"type":"message_start","message":{"usage":{"input_tokens":12,"output_tokens":1}}}),
        )
    }

    async fn collect(server: &mockito::ServerGuard) -> Vec<Result<GenerationEvent>> {
        let g = AnthropicGenerator::new("m", "k", Some(server.url()));
        g.stream(req()).await.unwrap().collect().await
    }

    #[tokio::test]
    async fn anthropic_streams_text_and_done() {
        let mut server = mockito::Server::new_async().await;
        let _m = server
            .mock("POST", "/v1/messages")
            .match_header("x-api-key", "k")
            .match_header("anthropic-version", "2023-06-01")
            .match_body(Matcher::PartialJson(
                json!({"model":"m","system":"sys","max_tokens":64,"stream":true}),
            ))
            .with_header("content-type", "text/event-stream")
            .with_body(start() + &delta("Hello") + &delta(" world") + &end("end_turn"))
            .create_async()
            .await;
        let got: Vec<_> = collect(&server)
            .await
            .into_iter()
            .map(|r| r.unwrap())
            .collect();
        assert_eq!(
            got,
            vec![
                GenerationEvent::TextDelta("Hello".into()),
                GenerationEvent::TextDelta(" world".into()),
                GenerationEvent::Done {
                    usage: GenerationUsage {
                        input_tokens: Some(12),
                        output_tokens: Some(5)
                    },
                    stop_reason: StopReason::EndTurn,
                },
            ]
        );
    }

    #[tokio::test]
    async fn anthropic_maps_stop_reasons() {
        for (raw, want) in [
            ("max_tokens", StopReason::MaxTokens),
            ("refusal", StopReason::Other("refusal".into())),
        ] {
            let mut server = mockito::Server::new_async().await;
            let _m = server
                .mock("POST", "/v1/messages")
                .with_header("content-type", "text/event-stream")
                .with_body(start() + &delta("x") + &end(raw))
                .create_async()
                .await;
            let got = collect(&server).await;
            match got.last().unwrap().as_ref().unwrap() {
                GenerationEvent::Done { stop_reason, .. } => assert_eq!(stop_reason, &want),
                other => panic!("unexpected {other:?}"),
            }
        }
    }

    #[tokio::test]
    async fn anthropic_non_2xx_is_error() {
        let mut server = mockito::Server::new_async().await;
        let _m = server
            .mock("POST", "/v1/messages")
            .with_status(401)
            .with_body("bad key")
            .create_async()
            .await;
        let g = AnthropicGenerator::new("m", "k", Some(server.url()));
        let err = g.stream(req()).await.err().expect("should be Err");
        assert!(err.to_string().contains("401"), "{err}");
    }

    #[tokio::test]
    async fn anthropic_error_event_mid_stream() {
        let mut server = mockito::Server::new_async().await;
        let body = start()
            + &delta("Hi")
            + &ev(
                "error",
                json!({"type":"error","error":{"type":"overloaded_error","message":"Overloaded"}}),
            )
            + &delta("never");
        let _m = server
            .mock("POST", "/v1/messages")
            .with_header("content-type", "text/event-stream")
            .with_body(body)
            .create_async()
            .await;
        let got = collect(&server).await;
        assert_eq!(got.len(), 2);
        assert_eq!(
            got[0].as_ref().unwrap(),
            &GenerationEvent::TextDelta("Hi".into())
        );
        assert!(got[1]
            .as_ref()
            .unwrap_err()
            .to_string()
            .contains("Overloaded"));
    }

    #[tokio::test]
    async fn anthropic_stream_without_stop_is_error() {
        let mut server = mockito::Server::new_async().await;
        let _m = server
            .mock("POST", "/v1/messages")
            .with_header("content-type", "text/event-stream")
            .with_body(start() + &delta("Hi"))
            .create_async()
            .await;
        let got = collect(&server).await;
        assert!(got
            .last()
            .unwrap()
            .as_ref()
            .unwrap_err()
            .to_string()
            .contains("stream ended before completion"));
    }
}
