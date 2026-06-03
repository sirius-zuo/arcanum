use arcanum_core::{traits::*, types::*, Result, ArcanumError};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tracing::instrument;
use metrics;

pub struct OpenAiProvider {
    api_key: String,
    pub embed_model: String,
    pub generate_model: String,
    client: reqwest::Client,
}

impl OpenAiProvider {
    pub fn new(api_key: &str, embed_model: &str, generate_model: &str) -> Self {
        Self {
            api_key: api_key.to_string(),
            embed_model: embed_model.to_string(),
            generate_model: generate_model.to_string(),
            client: reqwest::Client::new(),
        }
    }
}

#[derive(Serialize)]
struct OaiEmbedRequest<'a> {
    input: &'a str,
    model: &'a str,
}

#[derive(Deserialize)]
struct OaiEmbedResponse {
    data: Vec<OaiEmbedData>,
}

#[derive(Deserialize)]
struct OaiEmbedData {
    embedding: Vec<f32>,
}

#[derive(Serialize)]
struct OaiChatRequest<'a> {
    model: &'a str,
    messages: Vec<OaiMessage<'a>>,
}

#[derive(Serialize)]
struct OaiMessage<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Deserialize)]
struct OaiChatResponse {
    choices: Vec<OaiChoice>,
}

#[derive(Deserialize)]
struct OaiChoice {
    message: OaiMessageContent,
}

#[derive(Deserialize)]
struct OaiMessageContent {
    content: String,
}

#[async_trait]
impl Embedder for OpenAiProvider {
    #[instrument(skip(self, texts), fields(model = %self.embed_model, text_count = texts.len(), dimension), err)]
    async fn embed(&self, texts: Vec<String>) -> Result<Vec<Vector>> {
        let start = std::time::Instant::now();
        let mut results = Vec::new();
        for text in &texts {
            let resp: OaiEmbedResponse = self.client
                .post("https://api.openai.com/v1/embeddings")
                .bearer_auth(&self.api_key)
                .json(&OaiEmbedRequest { input: text, model: &self.embed_model })
                .send().await.map_err(|e| ArcanumError::Embedding(e.to_string()))?
                .json().await.map_err(|e| ArcanumError::Embedding(e.to_string()))?;
            if let Some(d) = resp.data.into_iter().next() {
                results.push(Vector(d.embedding));
            }
        }
        tracing::Span::current().record("dimension", self.dimension());
        metrics::counter!("arcanum_model_calls_total", "provider" => "openai", "operation" => "embed", "status" => "ok").increment(1);
        metrics::histogram!("arcanum_model_call_duration_seconds", "provider" => "openai", "operation" => "embed").record(start.elapsed().as_secs_f64());
        Ok(results)
    }

    fn dimension(&self) -> usize {
        match self.embed_model.as_str() {
            "text-embedding-3-large" => 3072,
            "text-embedding-3-small" | "text-embedding-ada-002" => 1536,
            _ => 1536,
        }
    }
}

#[async_trait]
impl TextEnricher for OpenAiProvider {
    #[instrument(skip(self, request), fields(model = %self.generate_model, intent = ?request.intent), err)]
    async fn enrich(&self, request: EnrichRequest) -> Result<EnrichedText> {
        let start = std::time::Instant::now();
        let prompt = crate::ollama::build_prompt_for_enricher(&request);
        let result = self.client
            .post("https://api.openai.com/v1/chat/completions")
            .bearer_auth(&self.api_key)
            .json(&OaiChatRequest {
                model: &self.generate_model,
                messages: vec![OaiMessage { role: "user", content: &prompt }],
            })
            .send().await.map_err(|e| ArcanumError::Enrichment(e.to_string()))?
            .json::<OaiChatResponse>().await.map_err(|e| ArcanumError::Enrichment(e.to_string()));
        let result = result.map(|resp| {
            let text = resp.choices.into_iter().next()
                .map(|c| c.message.content)
                .unwrap_or_default();
            EnrichedText(text)
        });
        let status = if result.is_ok() { "ok" } else { "error" };
        metrics::counter!("arcanum_model_calls_total", "provider" => "openai", "operation" => "enrich", "status" => status).increment(1);
        metrics::histogram!("arcanum_model_call_duration_seconds", "provider" => "openai", "operation" => "enrich").record(start.elapsed().as_secs_f64());
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_openai_provider_construction() {
        let p = OpenAiProvider::new("sk-fake-key", "text-embedding-3-small", "gpt-4o");
        assert_eq!(p.embed_model, "text-embedding-3-small");
        assert_eq!(p.generate_model, "gpt-4o");
    }

    #[test]
    fn test_openai_dimension() {
        let p = OpenAiProvider::new("k", "text-embedding-3-large", "gpt-4o");
        assert_eq!(p.dimension(), 3072);
        let p2 = OpenAiProvider::new("k", "text-embedding-3-small", "gpt-4o");
        assert_eq!(p2.dimension(), 1536);
    }
}
