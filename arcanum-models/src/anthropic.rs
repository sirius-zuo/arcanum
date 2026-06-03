use arcanum_core::{traits::TextEnricher, types::*, Result, ArcanumError};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tracing::instrument;
use metrics;

pub struct AnthropicProvider {
    api_key: String,
    pub model: String,
    client: reqwest::Client,
}

impl AnthropicProvider {
    pub fn new(api_key: &str, model: &str) -> Self {
        Self {
            api_key: api_key.to_string(),
            model: model.to_string(),
            client: reqwest::Client::new(),
        }
    }
}

#[derive(Serialize)]
struct AnthropicRequest<'a> {
    model: &'a str,
    max_tokens: u32,
    messages: Vec<AnthropicMessage<'a>>,
}

#[derive(Serialize)]
struct AnthropicMessage<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Deserialize)]
struct AnthropicResponse {
    content: Vec<AnthropicContent>,
}

#[derive(Deserialize)]
struct AnthropicContent {
    text: String,
}

#[async_trait]
impl TextEnricher for AnthropicProvider {
    #[instrument(skip(self, request), fields(model = %self.model, intent = ?request.intent), err)]
    async fn enrich(&self, request: EnrichRequest) -> Result<EnrichedText> {
        let start = std::time::Instant::now();
        let prompt = crate::ollama::build_prompt_for_enricher(&request);
        let result = self.client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&AnthropicRequest {
                model: &self.model,
                max_tokens: 1024,
                messages: vec![AnthropicMessage { role: "user", content: &prompt }],
            })
            .send().await.map_err(|e| ArcanumError::Enrichment(e.to_string()))?
            .json::<AnthropicResponse>().await.map_err(|e| ArcanumError::Enrichment(e.to_string()));
        let result = result.map(|resp| {
            let text = resp.content.into_iter().next()
                .map(|c| c.text)
                .unwrap_or_default();
            EnrichedText(text)
        });
        let status = if result.is_ok() { "ok" } else { "error" };
        metrics::counter!("arcanum_model_calls_total", "provider" => "anthropic", "operation" => "enrich", "status" => status).increment(1);
        metrics::histogram!("arcanum_model_call_duration_seconds", "provider" => "anthropic", "operation" => "enrich").record(start.elapsed().as_secs_f64());
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_anthropic_provider_construction() {
        let p = AnthropicProvider::new("sk-ant-fake", "claude-haiku-4-5-20251001");
        assert_eq!(p.model, "claude-haiku-4-5-20251001");
    }
}
