use arcanum_core::{traits::*, types::*, Result, ArcanumError};
use async_trait::async_trait;
use tracing::instrument;
use metrics;

/// LLM2Vec: decoder LLM repurposed for embeddings and text enrichment via a local server.
pub struct Llm2VecProvider {
    pub base_url: String,
    pub dim: usize,
    client: reqwest::Client,
}

impl Llm2VecProvider {
    pub fn new(base_url: &str, dimension: usize) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            dim: dimension,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl Embedder for Llm2VecProvider {
    #[instrument(skip(self, texts), fields(model = %self.base_url, text_count = texts.len(), dimension), err)]
    async fn embed(&self, texts: Vec<String>) -> Result<Vec<Vector>> {
        let start = std::time::Instant::now();
        let result: Result<Vec<Vector>> = async {
            let mut results = Vec::new();
            for text in &texts {
                let resp: Vec<Vec<f32>> = self.client
                    .post(format!("{}/embed", self.base_url))
                    .json(&serde_json::json!({ "inputs": text }))
                    .send().await.map_err(|e| ArcanumError::Embedding(e.to_string()))?
                    .json().await.map_err(|e| ArcanumError::Embedding(e.to_string()))?;
                if let Some(v) = resp.into_iter().next() {
                    results.push(Vector(v));
                }
            }
            tracing::Span::current().record("dimension", self.dim);
            Ok(results)
        }.await;
        let status = if result.is_ok() { "ok" } else { "error" };
        metrics::counter!("arcanum_model_calls_total", "provider" => "llm2vec", "operation" => "embed", "status" => status).increment(1);
        metrics::histogram!("arcanum_model_call_duration_seconds", "provider" => "llm2vec", "operation" => "embed").record(start.elapsed().as_secs_f64());
        result
    }

    fn dimension(&self) -> usize {
        self.dim
    }
}

#[async_trait]
impl TextEnricher for Llm2VecProvider {
    #[instrument(skip(self, request), fields(model = %self.base_url, intent = ?request.intent), err)]
    async fn enrich(&self, request: EnrichRequest) -> Result<EnrichedText> {
        let start = std::time::Instant::now();
        let prompt = crate::ollama::build_prompt_for_enricher(&request);
        let result = self.client
            .post(format!("{}/generate", self.base_url))
            .json(&serde_json::json!({ "prompt": prompt }))
            .send().await.map_err(|e| ArcanumError::Enrichment(e.to_string()))?
            .json().await.map_err(|e| ArcanumError::Enrichment(e.to_string()));
        let result = result.map(|resp: serde_json::Value| {
            EnrichedText(resp["response"].as_str().unwrap_or("").to_string())
        });
        let status = if result.is_ok() { "ok" } else { "error" };
        metrics::counter!("arcanum_model_calls_total", "provider" => "llm2vec", "operation" => "enrich", "status" => status).increment(1);
        metrics::histogram!("arcanum_model_call_duration_seconds", "provider" => "llm2vec", "operation" => "enrich").record(start.elapsed().as_secs_f64());
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_llm2vec_construction() {
        let p = Llm2VecProvider::new("http://localhost:8082", 4096);
        assert_eq!(p.dim, 4096);
    }
}
