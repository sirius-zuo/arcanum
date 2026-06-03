use arcanum_core::{traits::Embedder, types::*, Result, ArcanumError};
use async_trait::async_trait;
use serde::Serialize;
use tracing::instrument;

pub struct HuggingFaceTeiProvider {
    pub base_url: String,
    pub model_id: String,
    pub dim: usize,
    client: reqwest::Client,
}

impl HuggingFaceTeiProvider {
    pub fn new(base_url: &str, model_id: &str, dimension: usize) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            model_id: model_id.to_string(),
            dim: dimension,
            client: reqwest::Client::new(),
        }
    }
}

#[derive(Serialize)]
struct TeiEmbedRequest<'a> {
    inputs: &'a str,
}

#[async_trait]
impl Embedder for HuggingFaceTeiProvider {
    #[instrument(skip(self, texts), fields(model = %self.model_id, text_count = texts.len(), dimension), err)]
    async fn embed(&self, texts: Vec<String>) -> Result<Vec<Vector>> {
        let mut results = Vec::new();
        for text in &texts {
            let resp: Vec<Vec<f32>> = self.client
                .post(format!("{}/embed", self.base_url))
                .json(&TeiEmbedRequest { inputs: text })
                .send().await.map_err(|e| ArcanumError::Embedding(e.to_string()))?
                .json().await.map_err(|e| ArcanumError::Embedding(e.to_string()))?;
            if let Some(v) = resp.into_iter().next() {
                results.push(Vector(v));
            }
        }
        tracing::Span::current().record("dimension", self.dim);
        Ok(results)
    }

    fn dimension(&self) -> usize {
        self.dim
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tei_provider_construction() {
        let p = HuggingFaceTeiProvider::new("http://localhost:8080", "BAAI/bge-large-en-v1.5", 1024);
        assert_eq!(p.dim, 1024);
        assert_eq!(p.model_id, "BAAI/bge-large-en-v1.5");
    }
}
