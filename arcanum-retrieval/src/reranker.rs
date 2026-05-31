use arcanum_core::{traits::*, types::*, Result};
use async_trait::async_trait;
use std::sync::Arc;

// ---------------------------------------------------------------------------
// NullReranker — passthrough
// ---------------------------------------------------------------------------

/// Passthrough reranker: returns chunks in the same order and with the same
/// scores as received. Useful as a default/no-op.
pub struct NullReranker;

#[async_trait]
impl Reranker for NullReranker {
    async fn rerank(&self, _query: &Query, chunks: Vec<RetrievedChunk>) -> Result<Vec<RetrievedChunk>> {
        Ok(chunks)
    }
}

// ---------------------------------------------------------------------------
// ScoreFusionReranker — sort by score descending
// ---------------------------------------------------------------------------

/// Reranker that simply sorts chunks by their existing score (descending).
/// Useful when multiple fusion strategies have produced raw scores that need
/// a final canonical ordering.
pub struct ScoreFusionReranker;

#[async_trait]
impl Reranker for ScoreFusionReranker {
    async fn rerank(&self, _query: &Query, mut chunks: Vec<RetrievedChunk>) -> Result<Vec<RetrievedChunk>> {
        chunks.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        Ok(chunks)
    }
}

// ---------------------------------------------------------------------------
// LlmReranker — calls a TextEnricher with EnrichIntent::Rerank
// ---------------------------------------------------------------------------

/// Reranker that uses an LLM (via TextEnricher) to score each chunk's
/// relevance to the query. The enricher is called once per chunk with
/// `EnrichIntent::Rerank`; the response text is parsed as a float score.
/// Chunks that cannot be scored fall back to their original score.
pub struct LlmReranker {
    enricher: Arc<dyn TextEnricher>,
}

impl LlmReranker {
    pub fn new(enricher: Arc<dyn TextEnricher>) -> Self {
        Self { enricher }
    }
}

#[async_trait]
impl Reranker for LlmReranker {
    async fn rerank(&self, query: &Query, chunks: Vec<RetrievedChunk>) -> Result<Vec<RetrievedChunk>> {
        let mut scored = Vec::with_capacity(chunks.len());
        for mut chunk in chunks {
            let prompt = format!(
                "Query: {}\nChunk: {}\nRate relevance 0.0-1.0:",
                query.text, chunk.indexed_chunk.chunk.text
            );
            let req = EnrichRequest {
                text: prompt,
                intent: EnrichIntent::Rerank,
                context: None,
            };
            if let Ok(enriched) = self.enricher.enrich(req).await {
                if let Ok(score) = enriched.0.trim().parse::<f32>() {
                    chunk.score = score;
                }
            }
            scored.push(chunk);
        }
        scored.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        Ok(scored)
    }
}

// ---------------------------------------------------------------------------
// CrossEncoderReranker — HTTP call to a cross-encoder scoring service
// ---------------------------------------------------------------------------

/// Reranker that calls an external cross-encoder HTTP API.
/// POST `{base_url}/rerank` with JSON `{"query": "...", "texts": ["..."]}`.
/// Expects a JSON array of scores back: `[0.9, 0.3, ...]`.
pub struct CrossEncoderReranker {
    base_url: String,
    client: reqwest::Client,
}

impl CrossEncoderReranker {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl Reranker for CrossEncoderReranker {
    async fn rerank(&self, query: &Query, mut chunks: Vec<RetrievedChunk>) -> Result<Vec<RetrievedChunk>> {
        if chunks.is_empty() { return Ok(chunks); }

        let texts: Vec<&str> = chunks.iter()
            .map(|c| c.indexed_chunk.chunk.text.as_str())
            .collect();

        let body = serde_json::json!({ "query": query.text, "texts": texts });
        let url = format!("{}/rerank", self.base_url);

        let resp = self.client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| arcanum_core::ArcanumError::Config(format!("CrossEncoder HTTP error: {e}")))?;

        let scores: Vec<f32> = resp
            .json()
            .await
            .map_err(|e| arcanum_core::ArcanumError::Config(format!("CrossEncoder parse error: {e}")))?;

        for (chunk, score) in chunks.iter_mut().zip(scores.iter()) {
            chunk.score = *score;
        }
        chunks.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        Ok(chunks)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_chunk(score: f32, text: &str) -> RetrievedChunk {
        RetrievedChunk {
            indexed_chunk: IndexedChunk {
                chunk: Chunk {
                    id: ChunkId::new(),
                    text: text.to_string(),
                    document_id: DocumentId::new(),
                    collection_id: CollectionId("col".into()),
                    position: ChunkPosition { start: 0, end: text.len(), index: 0 },
                    metadata: ChunkMetadata::default(),
                },
                vector: Vector(vec![]),
                token_vectors: None,
                store_id: "".into(),
            },
            score,
            strategy: RetrievalStrategy::Vector,
        }
    }

    #[tokio::test]
    async fn test_null_reranker_passthrough() {
        let r = NullReranker;
        let q = Query::new("test");
        let chunks = vec![make_chunk(0.5, "a"), make_chunk(0.9, "b")];
        let result = r.rerank(&q, chunks.clone()).await.unwrap();
        assert_eq!(result.len(), 2);
        // Order unchanged
        assert_eq!(result[0].score, chunks[0].score);
        assert_eq!(result[1].score, chunks[1].score);
    }

    #[tokio::test]
    async fn test_score_fusion_reranker_sorts_descending() {
        let r = ScoreFusionReranker;
        let q = Query::new("test");
        let chunks = vec![make_chunk(0.3, "low"), make_chunk(0.9, "high"), make_chunk(0.6, "mid")];
        let result = r.rerank(&q, chunks).await.unwrap();
        assert_eq!(result.len(), 3);
        assert!(result[0].score >= result[1].score);
        assert!(result[1].score >= result[2].score);
        assert_eq!(result[0].indexed_chunk.chunk.text, "high");
    }
}
