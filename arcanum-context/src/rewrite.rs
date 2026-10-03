use std::sync::Arc;

use arcanum_core::traits::TextEnricher;
use arcanum_core::types::*;
use arcanum_core::{ArcanumError, Result};
use async_trait::async_trait;
use tracing::warn;

const MAX_MESSAGE_CHARS: usize = 2000;
const MAX_REWRITE_CHARS: usize = 1000;

#[async_trait]
pub trait ConversationRewriter: Send + Sync {
    async fn rewrite(&self, messages: &[Message]) -> Result<String>;
}

pub struct EnricherRewriter {
    enricher: Arc<dyn TextEnricher>,
    max_messages: usize,
}

impl EnricherRewriter {
    pub fn new(enricher: Arc<dyn TextEnricher>, max_messages: usize) -> Self {
        Self {
            enricher,
            max_messages,
        }
    }
}

#[async_trait]
impl ConversationRewriter for EnricherRewriter {
    async fn rewrite(&self, messages: &[Message]) -> Result<String> {
        let start = messages.len().saturating_sub(self.max_messages);
        let text = messages[start..]
            .iter()
            .map(|m| {
                let who = match m.role {
                    Role::User => "User",
                    Role::Assistant => "Assistant",
                };
                let content: String = m.content.chars().take(MAX_MESSAGE_CHARS).collect();
                format!("{who}: {content}")
            })
            .collect::<Vec<_>>()
            .join("\n");
        let out = self
            .enricher
            .enrich(EnrichRequest {
                text,
                intent: EnrichIntent::RewriteQuery,
                context: None,
            })
            .await
            .map_err(|e| ArcanumError::Enrichment(e.to_string()))?;
        Ok(out.0.trim().to_string())
    }
}

#[derive(Debug, Clone)]
pub struct ResolvedQuery {
    pub text: String,
    pub source: ResolvedQuerySource,
}

fn last_user_message(messages: &[Message]) -> String {
    messages
        .iter()
        .rev()
        .find(|m| m.role == Role::User)
        .map(|m| m.content.clone())
        .unwrap_or_default()
}

/// Input is assumed already validated (exactly one of query/messages present).
pub async fn resolve_query(
    query: Option<&str>,
    messages: Option<&[Message]>,
    rewriter: Option<&dyn ConversationRewriter>,
) -> ResolvedQuery {
    if let Some(q) = query {
        return ResolvedQuery {
            text: q.to_string(),
            source: ResolvedQuerySource::Original,
        };
    }
    let messages = messages.unwrap_or_default();
    if messages.len() <= 1 {
        return ResolvedQuery {
            text: messages
                .first()
                .map(|m| m.content.clone())
                .unwrap_or_default(),
            source: ResolvedQuerySource::Original,
        };
    }
    let fallback = || ResolvedQuery {
        text: last_user_message(messages),
        source: ResolvedQuerySource::Fallback,
    };
    let Some(rewriter) = rewriter else {
        return fallback();
    };
    match rewriter.rewrite(messages).await {
        Ok(text) => {
            let text = text.trim().to_string();
            if text.is_empty() || text.chars().count() > MAX_REWRITE_CHARS {
                warn!("query rewrite empty or too long, using last user message");
                fallback()
            } else {
                ResolvedQuery {
                    text,
                    source: ResolvedQuerySource::Rewritten,
                }
            }
        }
        Err(e) => {
            warn!(error = %e, "query rewrite failed, using last user message");
            fallback()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    fn msg(role: Role, c: &str) -> Message {
        Message {
            role,
            content: c.to_string(),
        }
    }

    struct FakeRewriter {
        out: std::result::Result<String, String>,
        calls: Mutex<usize>,
    }

    impl FakeRewriter {
        fn new(out: std::result::Result<String, String>) -> Self {
            Self {
                out,
                calls: Mutex::new(0),
            }
        }
        fn calls(&self) -> usize {
            *self.calls.lock().unwrap()
        }
    }

    #[async_trait]
    impl ConversationRewriter for FakeRewriter {
        async fn rewrite(&self, _m: &[Message]) -> Result<String> {
            *self.calls.lock().unwrap() += 1;
            self.out.clone().map_err(ArcanumError::Enrichment)
        }
    }

    struct RecordingEnricher {
        seen: Mutex<Vec<EnrichRequest>>,
    }

    #[async_trait]
    impl TextEnricher for RecordingEnricher {
        async fn enrich(&self, request: EnrichRequest) -> Result<EnrichedText> {
            self.seen.lock().unwrap().push(request);
            Ok(EnrichedText("  rewritten \n".into()))
        }
    }

    fn convo() -> Vec<Message> {
        vec![
            msg(Role::User, "first"),
            msg(Role::Assistant, "answer"),
            msg(Role::User, "and the second?"),
        ]
    }

    #[tokio::test]
    async fn plain_query_is_original_without_call() {
        let rw = FakeRewriter::new(Ok("x".into()));
        let r = resolve_query(Some("hello"), None, Some(&rw)).await;
        assert_eq!(r.text, "hello");
        assert_eq!(r.source, ResolvedQuerySource::Original);
        assert_eq!(rw.calls(), 0);
    }

    #[tokio::test]
    async fn single_message_is_original_without_call() {
        let rw = FakeRewriter::new(Ok("x".into()));
        let m = vec![msg(Role::User, "only")];
        let r = resolve_query(None, Some(&m), Some(&rw)).await;
        assert_eq!(r.text, "only");
        assert_eq!(r.source, ResolvedQuerySource::Original);
        assert_eq!(rw.calls(), 0);
    }

    #[tokio::test]
    async fn multi_message_is_rewritten() {
        let rw = FakeRewriter::new(Ok("standalone?".into()));
        let m = convo();
        let r = resolve_query(None, Some(&m), Some(&rw)).await;
        assert_eq!(r.text, "standalone?");
        assert_eq!(r.source, ResolvedQuerySource::Rewritten);
        assert_eq!(rw.calls(), 1);
    }

    #[tokio::test]
    async fn error_empty_and_overlong_fall_back() {
        let m = convo();
        for out in [
            Err("boom".to_string()),
            Ok("  ".into()),
            Ok("x".repeat(1001)),
        ] {
            let rw = FakeRewriter::new(out);
            let r = resolve_query(None, Some(&m), Some(&rw)).await;
            assert_eq!(r.text, "and the second?");
            assert_eq!(r.source, ResolvedQuerySource::Fallback);
        }
    }

    #[tokio::test]
    async fn no_rewriter_falls_back() {
        let m = convo();
        let r = resolve_query(None, Some(&m), None).await;
        assert_eq!(r.text, "and the second?");
        assert_eq!(r.source, ResolvedQuerySource::Fallback);
    }

    #[tokio::test]
    async fn enricher_rewriter_windows_and_truncates() {
        let enricher = Arc::new(RecordingEnricher {
            seen: Mutex::new(vec![]),
        });
        let rw = EnricherRewriter::new(enricher.clone(), 6);
        let mut m: Vec<Message> = (0..8)
            .map(|i| {
                msg(
                    if i % 2 == 0 {
                        Role::User
                    } else {
                        Role::Assistant
                    },
                    &format!("m{i}"),
                )
            })
            .collect();
        m[6].content = "y".repeat(3000);
        let out = rw.rewrite(&m).await.unwrap();
        assert_eq!(out, "rewritten");
        let seen = enricher.seen.lock().unwrap();
        assert!(matches!(seen[0].intent, EnrichIntent::RewriteQuery));
        assert!(seen[0].context.is_none());
        let lines: Vec<&str> = seen[0].text.split('\n').collect();
        assert_eq!(lines.len(), 6);
        assert_eq!(lines[0], "User: m2");
        assert_eq!(lines[4], format!("User: {}", "y".repeat(2000)));
    }
}
