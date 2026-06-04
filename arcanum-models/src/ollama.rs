use arcanum_core::{traits::*, types::*, Result, ArcanumError};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tracing::instrument;
use metrics;

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
    #[instrument(skip(self, texts), fields(model = %self.embed_model, text_count = texts.len(), dimension), err)]
    async fn embed(&self, texts: Vec<String>) -> Result<Vec<Vector>> {
        let start = std::time::Instant::now();
        let result: Result<Vec<Vector>> = async {
            let mut results = vec![];
            for text in &texts {
                let resp: EmbedResponse = self.client
                    .post(format!("{}/api/embeddings", self.base_url))
                    .json(&EmbedRequest { model: &self.embed_model, prompt: text })
                    .send().await.map_err(|e| ArcanumError::Embedding(e.to_string()))?
                    .json().await.map_err(|e| ArcanumError::Embedding(e.to_string()))?;
                results.push(Vector(resp.embedding));
            }
            tracing::Span::current().record("dimension", self.dimension());
            Ok(results)
        }.await;
        let status = if result.is_ok() { "ok" } else { "error" };
        metrics::counter!("arcanum_model_calls_total", "provider" => "ollama", "operation" => "embed", "status" => status).increment(1);
        metrics::histogram!("arcanum_model_call_duration_seconds", "provider" => "ollama", "operation" => "embed").record(start.elapsed().as_secs_f64());
        result
    }

    fn dimension(&self) -> usize { 0 } // determined at runtime from API response
}

#[async_trait]
impl TextEnricher for OllamaProvider {
    #[instrument(skip(self, request), fields(model = %self.generate_model, intent = ?request.intent), err)]
    async fn enrich(&self, request: EnrichRequest) -> Result<EnrichedText> {
        let start = std::time::Instant::now();
        let prompt = build_prompt_for_enricher(&request);
        let result = self.client
            .post(format!("{}/api/generate", self.base_url))
            .json(&GenerateRequest { model: self.generate_model.clone(), prompt, stream: false })
            .send().await.map_err(|e| ArcanumError::Enrichment(e.to_string()))?
            .json().await.map_err(|e| ArcanumError::Enrichment(e.to_string()));
        let result = result.map(|resp: GenerateResponse| EnrichedText(resp.response));
        let status = if result.is_ok() { "ok" } else { "error" };
        metrics::counter!("arcanum_model_calls_total", "provider" => "ollama", "operation" => "enrich", "status" => status).increment(1);
        metrics::histogram!("arcanum_model_call_duration_seconds", "provider" => "ollama", "operation" => "enrich").record(start.elapsed().as_secs_f64());
        result
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
