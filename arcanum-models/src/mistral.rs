use arcanum_core::{traits::*, types::*, Result, ArcanumError};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tracing::instrument;
use metrics;

pub struct MistralProvider {
    api_key: String,
    pub embed_model: String,
    pub generate_model: String,
    client: reqwest::Client,
}

impl MistralProvider {
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
struct MistralEmbedRequest<'a> {
    input: Vec<&'a str>,
    model: &'a str,
}

#[derive(Deserialize)]
struct MistralEmbedResponse {
    data: Vec<MistralEmbedData>,
}

#[derive(Deserialize)]
struct MistralEmbedData {
    embedding: Vec<f32>,
}

#[derive(Serialize)]
struct MistralChatRequest<'a> {
    model: &'a str,
    messages: Vec<MistralMsg<'a>>,
}

#[derive(Serialize)]
struct MistralMsg<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Deserialize)]
struct MistralChatResponse {
    choices: Vec<MistralChoice>,
}

#[derive(Deserialize)]
struct MistralChoice {
    message: MistralMsgContent,
}

#[derive(Deserialize)]
struct MistralMsgContent {
    content: String,
}

#[async_trait]
impl Embedder for MistralProvider {
    #[instrument(skip(self, texts), fields(model = %self.embed_model, text_count = texts.len(), dimension), err)]
    async fn embed(&self, texts: Vec<String>) -> Result<Vec<Vector>> {
        let start = std::time::Instant::now();
        let inputs: Vec<&str> = texts.iter().map(|s| s.as_str()).collect();
        let result: Result<Vec<Vector>> = async {
            let resp: MistralEmbedResponse = self.client
                .post("https://api.mistral.ai/v1/embeddings")
                .bearer_auth(&self.api_key)
                .json(&MistralEmbedRequest { input: inputs, model: &self.embed_model })
                .send().await.map_err(|e| ArcanumError::Embedding(e.to_string()))?
                .json().await.map_err(|e| ArcanumError::Embedding(e.to_string()))?;
            tracing::Span::current().record("dimension", self.dimension());
            Ok(resp.data.into_iter().map(|d| Vector(d.embedding)).collect())
        }.await;
        let status = if result.is_ok() { "ok" } else { "error" };
        metrics::counter!("arcanum_model_calls_total", "provider" => "mistral", "operation" => "embed", "status" => status).increment(1);
        metrics::histogram!("arcanum_model_call_duration_seconds", "provider" => "mistral", "operation" => "embed").record(start.elapsed().as_secs_f64());
        result
    }

    fn dimension(&self) -> usize {
        match self.embed_model.as_str() {
            "mistral-embed" => 1024,
            _ => 1024,
        }
    }
}

#[async_trait]
impl TextEnricher for MistralProvider {
    #[instrument(skip(self, request), fields(model = %self.generate_model, intent = ?request.intent), err)]
    async fn enrich(&self, request: EnrichRequest) -> Result<EnrichedText> {
        let start = std::time::Instant::now();
        let prompt = crate::ollama::build_prompt_for_enricher(&request);
        let result = self.client
            .post("https://api.mistral.ai/v1/chat/completions")
            .bearer_auth(&self.api_key)
            .json(&MistralChatRequest {
                model: &self.generate_model,
                messages: vec![MistralMsg { role: "user", content: &prompt }],
            })
            .send().await.map_err(|e| ArcanumError::Enrichment(e.to_string()))?
            .json::<MistralChatResponse>().await.map_err(|e| ArcanumError::Enrichment(e.to_string()));
        let result = result.map(|resp| {
            EnrichedText(
                resp.choices.into_iter().next()
                    .map(|c| c.message.content)
                    .unwrap_or_default()
            )
        });
        let status = if result.is_ok() { "ok" } else { "error" };
        metrics::counter!("arcanum_model_calls_total", "provider" => "mistral", "operation" => "enrich", "status" => status).increment(1);
        metrics::histogram!("arcanum_model_call_duration_seconds", "provider" => "mistral", "operation" => "enrich").record(start.elapsed().as_secs_f64());
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mistral_provider_construction() {
        let p = MistralProvider::new("key", "mistral-embed", "mistral-small-latest");
        assert_eq!(p.embed_model, "mistral-embed");
        assert_eq!(p.generate_model, "mistral-small-latest");
    }

    #[test]
    fn test_mistral_dimension() {
        let p = MistralProvider::new("key", "mistral-embed", "mistral-small-latest");
        assert_eq!(p.dimension(), 1024);
    }
}
