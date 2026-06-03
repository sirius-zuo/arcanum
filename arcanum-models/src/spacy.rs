use arcanum_core::{traits::TextEnricher, types::*, Result, ArcanumError};
use async_trait::async_trait;
use tracing::instrument;
use metrics;

/// spaCy NLP pipeline via a local HTTP server.
/// Implements TextEnricher for ExtractEntities intent only.
pub struct SpacyProvider {
    pub base_url: String,
    client: reqwest::Client,
}

impl SpacyProvider {
    pub fn new(base_url: &str) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl TextEnricher for SpacyProvider {
    #[instrument(skip(self, request), fields(provider = "spacy", intent = ?request.intent), err)]
    async fn enrich(&self, request: EnrichRequest) -> Result<EnrichedText> {
        let start = std::time::Instant::now();
        let result = if !matches!(request.intent, EnrichIntent::ExtractEntities) {
            Err(ArcanumError::Enrichment(
                "spaCy provider only supports ExtractEntities".to_string(),
            ))
        } else {
            let resp: serde_json::Value = self.client
                .post(format!("{}/process", self.base_url))
                .json(&serde_json::json!({ "text": request.text, "pipeline": "ner" }))
                .send().await.map_err(|e| ArcanumError::Enrichment(e.to_string()))?
                .json().await.map_err(|e| ArcanumError::Enrichment(e.to_string()))?;
            let text_str = resp["text"].as_str().unwrap_or("").to_string();
            let entities: Vec<serde_json::Value> = resp["ents"]
                .as_array()
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .map(|e| {
                    let start = e["start"].as_u64().unwrap_or(0) as usize;
                    let end = e["end"].as_u64().unwrap_or(0) as usize;
                    let name = text_str.get(start..end).unwrap_or("").to_string();
                    serde_json::json!({
                        "name": name,
                        "type": e["label"].as_str().unwrap_or("UNKNOWN")
                    })
                })
                .collect();
            Ok(EnrichedText(
                serde_json::json!({ "entities": entities, "relations": [] }).to_string(),
            ))
        };
        let status = if result.is_ok() { "ok" } else { "error" };
        metrics::counter!("arcanum_model_calls_total", "provider" => "spacy", "operation" => "enrich", "status" => status).increment(1);
        metrics::histogram!("arcanum_model_call_duration_seconds", "provider" => "spacy", "operation" => "enrich").record(start.elapsed().as_secs_f64());
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spacy_construction() {
        let p = SpacyProvider::new("http://localhost:8084");
        assert!(p.base_url.contains("8084"));
    }
}
