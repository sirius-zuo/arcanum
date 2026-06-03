use arcanum_core::{traits::TextEnricher, types::*, Result, ArcanumError};
use async_trait::async_trait;
use tracing::instrument;

/// GLiNER: lightweight entity extraction via a local `/ner` HTTP endpoint.
/// Implements TextEnricher for ExtractEntities intent only.
pub struct GlinerProvider {
    pub base_url: String,
    client: reqwest::Client,
}

impl GlinerProvider {
    pub fn new(base_url: &str) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl TextEnricher for GlinerProvider {
    #[instrument(skip(self, request), fields(provider = "gliner", intent = ?request.intent), err)]
    async fn enrich(&self, request: EnrichRequest) -> Result<EnrichedText> {
        if !matches!(request.intent, EnrichIntent::ExtractEntities) {
            return Err(ArcanumError::Enrichment(
                "GLiNER only supports ExtractEntities".to_string(),
            ));
        }
        let resp: serde_json::Value = self.client
            .post(format!("{}/ner", self.base_url))
            .json(&serde_json::json!({ "text": request.text }))
            .send().await.map_err(|e| ArcanumError::Enrichment(e.to_string()))?
            .json().await.map_err(|e| ArcanumError::Enrichment(e.to_string()))?;
        Ok(EnrichedText(resp.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gliner_construction() {
        let p = GlinerProvider::new("http://localhost:8083");
        assert!(p.base_url.contains("8083"));
    }
}
