use arcanum_core::{traits::TextEnricher, types::*, Result, ArcanumError};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

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
    async fn enrich(&self, request: EnrichRequest) -> Result<EnrichedText> {
        let prompt = crate::ollama::build_prompt_for_enricher(&request);
        let resp: AnthropicResponse = self.client
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
            .json().await.map_err(|e| ArcanumError::Enrichment(e.to_string()))?;
        let text = resp.content.into_iter().next()
            .map(|c| c.text)
            .unwrap_or_default();
        Ok(EnrichedText(text))
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
