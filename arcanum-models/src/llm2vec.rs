use arcanum_core::{traits::*, types::*, Result, ArcanumError};
use async_trait::async_trait;

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
    async fn embed(&self, texts: Vec<String>) -> Result<Vec<Vector>> {
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
        Ok(results)
    }

    fn dimension(&self) -> usize {
        self.dim
    }
}

#[async_trait]
impl TextEnricher for Llm2VecProvider {
    async fn enrich(&self, request: EnrichRequest) -> Result<EnrichedText> {
        let prompt = crate::ollama::build_prompt_for_enricher(&request);
        let resp: serde_json::Value = self.client
            .post(format!("{}/generate", self.base_url))
            .json(&serde_json::json!({ "prompt": prompt }))
            .send().await.map_err(|e| ArcanumError::Enrichment(e.to_string()))?
            .json().await.map_err(|e| ArcanumError::Enrichment(e.to_string()))?;
        Ok(EnrichedText(resp["response"].as_str().unwrap_or("").to_string()))
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
