use arcanum_core::{traits::*, types::*, Result, ArcanumError};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

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
    async fn embed(&self, texts: Vec<String>) -> Result<Vec<Vector>> {
        let inputs: Vec<&str> = texts.iter().map(|s| s.as_str()).collect();
        let resp: MistralEmbedResponse = self.client
            .post("https://api.mistral.ai/v1/embeddings")
            .bearer_auth(&self.api_key)
            .json(&MistralEmbedRequest { input: inputs, model: &self.embed_model })
            .send().await.map_err(|e| ArcanumError::Embedding(e.to_string()))?
            .json().await.map_err(|e| ArcanumError::Embedding(e.to_string()))?;
        Ok(resp.data.into_iter().map(|d| Vector(d.embedding)).collect())
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
    async fn enrich(&self, request: EnrichRequest) -> Result<EnrichedText> {
        let prompt = crate::ollama::build_prompt_for_enricher(&request);
        let resp: MistralChatResponse = self.client
            .post("https://api.mistral.ai/v1/chat/completions")
            .bearer_auth(&self.api_key)
            .json(&MistralChatRequest {
                model: &self.generate_model,
                messages: vec![MistralMsg { role: "user", content: &prompt }],
            })
            .send().await.map_err(|e| ArcanumError::Enrichment(e.to_string()))?
            .json().await.map_err(|e| ArcanumError::Enrichment(e.to_string()))?;
        Ok(EnrichedText(
            resp.choices.into_iter().next()
                .map(|c| c.message.content)
                .unwrap_or_default()
        ))
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
