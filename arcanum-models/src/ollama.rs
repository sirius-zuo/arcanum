use arcanum_core::{traits::*, types::*, Result, ArcanumError};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

pub struct OllamaProvider {
    base_url: String,
    embed_model: String,
    generate_model: String,
    client: reqwest::Client,
}

impl OllamaProvider {
    pub fn new(base_url: &str, embed_model: &str, generate_model: &str) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            embed_model: embed_model.to_string(),
            generate_model: generate_model.to_string(),
            client: reqwest::Client::new(),
        }
    }
}

#[derive(Serialize)]
struct EmbedRequest<'a> { model: &'a str, prompt: &'a str }

#[derive(Deserialize)]
struct EmbedResponse { embedding: Vec<f32> }

#[derive(Serialize)]
struct GenerateRequest { model: String, prompt: String, stream: bool }

#[derive(Deserialize)]
struct GenerateResponse { response: String }

#[async_trait]
impl Embedder for OllamaProvider {
    async fn embed(&self, texts: Vec<String>) -> Result<Vec<Vector>> {
        let mut results = vec![];
        for text in &texts {
            let resp: EmbedResponse = self.client
                .post(format!("{}/api/embeddings", self.base_url))
                .json(&EmbedRequest { model: &self.embed_model, prompt: text })
                .send().await.map_err(|e| ArcanumError::Embedding(e.to_string()))?
                .json().await.map_err(|e| ArcanumError::Embedding(e.to_string()))?;
            results.push(Vector(resp.embedding));
        }
        Ok(results)
    }

    fn dimension(&self) -> usize { 0 } // determined at runtime from API response
}

#[async_trait]
impl TextEnricher for OllamaProvider {
    async fn enrich(&self, request: EnrichRequest) -> Result<EnrichedText> {
        let prompt = build_prompt_for_enricher(&request);
        let resp: GenerateResponse = self.client
            .post(format!("{}/api/generate", self.base_url))
            .json(&GenerateRequest { model: self.generate_model.clone(), prompt, stream: false })
            .send().await.map_err(|e| ArcanumError::Enrichment(e.to_string()))?
            .json().await.map_err(|e| ArcanumError::Enrichment(e.to_string()))?;
        Ok(EnrichedText(resp.response))
    }
}

pub fn build_prompt_for_enricher(req: &EnrichRequest) -> String {
    match &req.intent {
        EnrichIntent::ContextPrefix => format!(
            "Generate a brief context sentence for this chunk that will help with retrieval. \
             Chunk: {}\nContext sentence:",
            req.text
        ),
        EnrichIntent::Summarize => format!("Summarize the following text concisely:\n{}", req.text),
        EnrichIntent::ExtractEntities => format!(
            "Extract named entities and relationships from the following text as JSON \
             {{\"entities\": [...], \"relations\": [...]}}: \n{}",
            req.text
        ),
        EnrichIntent::Caption => format!("Describe this image content: {}", req.text),
        EnrichIntent::Rerank => format!(
            "Rate the relevance of this passage to the query on a scale of 0-1. \
             Return only the number. Passage: {}", req.text
        ),
        EnrichIntent::Custom(prompt_prefix) => format!("{}\n{}", prompt_prefix, req.text),
    }
}
