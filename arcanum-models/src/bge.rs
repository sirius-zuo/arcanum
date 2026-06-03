use arcanum_core::{traits::Embedder, types::*, Result, ArcanumError};
use async_trait::async_trait;
use tracing::instrument;

/// BGE/E5 local embedding models served via a local HTTP endpoint (TEI-compatible).
pub struct BgeProvider {
    pub base_url: String,
    pub dim: usize,
    client: reqwest::Client,
}

impl BgeProvider {
    pub fn new(base_url: &str, dimension: usize) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            dim: dimension,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl Embedder for BgeProvider {
    #[instrument(skip(self, texts), fields(model = %self.base_url, text_count = texts.len(), dimension), err)]
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
    fn test_bge_provider_construction() {
        let p = BgeProvider::new("http://localhost:8081", 768);
        assert_eq!(p.dim, 768);
    }
}
