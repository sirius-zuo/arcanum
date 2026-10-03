use crate::types::Message;
use crate::{ArcanumError, Result};
use async_trait::async_trait;
use futures::stream::{self, BoxStream, StreamExt};
use serde::{Deserialize, Serialize, Serializer};
use std::sync::Mutex;

#[derive(Debug, Clone, PartialEq)]
pub struct GenerationRequest {
    pub system: String,
    pub messages: Vec<Message>,
    pub max_tokens: u32,
    pub temperature: Option<f32>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum GenerationEvent {
    TextDelta(String),
    Done {
        usage: GenerationUsage,
        stop_reason: StopReason,
    },
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct GenerationUsage {
    pub input_tokens: Option<u32>,
    pub output_tokens: Option<u32>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum StopReason {
    EndTurn,
    MaxTokens,
    Other(String),
}

impl Serialize for StopReason {
    fn serialize<S: Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.serialize_str(match self {
            StopReason::EndTurn => "end_turn",
            StopReason::MaxTokens => "max_tokens",
            StopReason::Other(_) => "other",
        })
    }
}

/// A streaming text generator. A stream ends with exactly one `Done` or one `Err`.
#[async_trait]
pub trait Generator: Send + Sync {
    async fn stream(
        &self,
        req: GenerationRequest,
    ) -> Result<BoxStream<'static, Result<GenerationEvent>>>;
    fn model(&self) -> &str;
}

#[derive(Debug, Clone)]
pub enum ScriptStep {
    Delta(String),
    Done(StopReason),
    Fail(String),
    Hang,
}

/// Test double that replays a fixed script of steps.
pub struct ScriptedGenerator {
    model: String,
    scripts: Vec<Vec<ScriptStep>>,
    calls: Mutex<usize>,
    last_request: Mutex<Option<GenerationRequest>>,
}

impl ScriptedGenerator {
    pub fn new(model: &str, steps: Vec<ScriptStep>) -> Self {
        Self::with_scripts(model, vec![steps])
    }

    /// Call `n` (0-based) replays `scripts[min(n, len - 1)]`.
    pub fn with_scripts(model: &str, scripts: Vec<Vec<ScriptStep>>) -> Self {
        assert!(!scripts.is_empty(), "at least one script is required");
        Self {
            model: model.to_string(),
            scripts,
            calls: Mutex::new(0),
            last_request: Mutex::new(None),
        }
    }

    pub fn calls(&self) -> usize {
        *self.calls.lock().unwrap()
    }

    pub fn last_request(&self) -> Option<GenerationRequest> {
        self.last_request.lock().unwrap().clone()
    }
}

#[async_trait]
impl Generator for ScriptedGenerator {
    async fn stream(
        &self,
        req: GenerationRequest,
    ) -> Result<BoxStream<'static, Result<GenerationEvent>>> {
        let n = {
            let mut c = self.calls.lock().unwrap();
            *c += 1;
            *c - 1
        };
        let steps = &self.scripts[n.min(self.scripts.len() - 1)];
        *self.last_request.lock().unwrap() = Some(req);
        let mut items: Vec<Result<GenerationEvent>> = Vec::new();
        let mut hang = false;
        for step in steps {
            match step {
                ScriptStep::Delta(t) => items.push(Ok(GenerationEvent::TextDelta(t.clone()))),
                ScriptStep::Done(r) => items.push(Ok(GenerationEvent::Done {
                    usage: GenerationUsage {
                        input_tokens: Some(10),
                        output_tokens: Some(5),
                    },
                    stop_reason: r.clone(),
                })),
                ScriptStep::Fail(m) => items.push(Err(ArcanumError::Generation(m.clone()))),
                ScriptStep::Hang => {
                    hang = true;
                    break;
                }
            }
        }
        let s = stream::iter(items);
        if hang {
            Ok(s.chain(stream::pending()).boxed())
        } else {
            Ok(s.boxed())
        }
    }

    fn model(&self) -> &str {
        &self.model
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Role;

    #[tokio::test]
    async fn scripted_generator_replays_steps() {
        let g = ScriptedGenerator::new(
            "m",
            vec![
                ScriptStep::Delta("a".into()),
                ScriptStep::Done(StopReason::EndTurn),
            ],
        );
        let req = GenerationRequest {
            system: "s".into(),
            messages: vec![Message {
                role: Role::User,
                content: "hi".into(),
            }],
            max_tokens: 10,
            temperature: None,
        };
        let events: Vec<_> = g
            .stream(req.clone())
            .await
            .unwrap()
            .map(|e| e.unwrap())
            .collect()
            .await;
        assert_eq!(
            events,
            vec![
                GenerationEvent::TextDelta("a".into()),
                GenerationEvent::Done {
                    usage: GenerationUsage {
                        input_tokens: Some(10),
                        output_tokens: Some(5)
                    },
                    stop_reason: StopReason::EndTurn
                }
            ]
        );
        assert_eq!(g.calls(), 1);
        assert_eq!(g.last_request(), Some(req));
        assert_eq!(g.model(), "m");
    }

    #[tokio::test]
    async fn with_scripts_advances_per_call() {
        let g = ScriptedGenerator::with_scripts(
            "m",
            vec![
                vec![
                    ScriptStep::Delta("a".into()),
                    ScriptStep::Done(StopReason::EndTurn),
                ],
                vec![
                    ScriptStep::Delta("b".into()),
                    ScriptStep::Done(StopReason::EndTurn),
                ],
            ],
        );
        let mut got = Vec::new();
        for _ in 0..3 {
            let req = GenerationRequest {
                system: "s".into(),
                messages: vec![],
                max_tokens: 10,
                temperature: None,
            };
            let events: Vec<_> = g.stream(req).await.unwrap().collect().await;
            match events[0].as_ref().unwrap() {
                GenerationEvent::TextDelta(t) => got.push(t.clone()),
                other => panic!("unexpected {other:?}"),
            }
        }
        assert_eq!(got, vec!["a", "b", "b"]);
        assert_eq!(g.calls(), 3);
    }
}
