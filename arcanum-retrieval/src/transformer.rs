use arcanum_core::{traits::*, types::*, Result};
use async_trait::async_trait;
use std::sync::Arc;
use tracing::instrument;

/// Transform a single query into one or more queries for multi-strategy retrieval.
#[async_trait]
pub trait QueryTransformer: Send + Sync {
    async fn transform(&self, query: Query) -> Result<Vec<Query>>;
}

// ---------------------------------------------------------------------------
// HyDE — Hypothetical Document Embeddings
// ---------------------------------------------------------------------------

/// Generates a hypothetical answer to the query, then uses it as an additional
/// query vector. Returns [original_query, hypothetical_answer_query].
pub struct HydeTransformer {
    enricher: Arc<dyn TextEnricher>,
}

impl HydeTransformer {
    pub fn new(enricher: Arc<dyn TextEnricher>) -> Self {
        Self { enricher }
    }
}

#[async_trait]
impl QueryTransformer for HydeTransformer {
    #[instrument(skip(self, query), fields(query_text_len = query.text.len()), err)]
    async fn transform(&self, query: Query) -> Result<Vec<Query>> {
        let req = EnrichRequest {
            text: query.text.clone(),
            intent: EnrichIntent::Custom("hyde_hypothetical_answer".to_string()),
            context: None,
        };
        let enriched = self.enricher.enrich(req).await?;
        let hypo = Query {
            text: enriched.0,
            collection_id: query.collection_id.clone(),
            top_k: query.top_k,
            filters: query.filters.clone(),
        };
        Ok(vec![query, hypo])
    }
}

// ---------------------------------------------------------------------------
// MultiQuery — generate n rephrased variants
// ---------------------------------------------------------------------------

/// Generates `n_variants` rephrased versions of the query.
/// Each variant is a separate Query with the same collection/top_k/filters.
pub struct MultiQueryTransformer {
    enricher: Arc<dyn TextEnricher>,
    n_variants: usize,
}

impl MultiQueryTransformer {
    pub fn new(enricher: Arc<dyn TextEnricher>, n_variants: usize) -> Self {
        Self { enricher, n_variants }
    }
}

#[async_trait]
impl QueryTransformer for MultiQueryTransformer {
    #[instrument(skip(self, query), fields(n_variants = self.n_variants, query_text_len = query.text.len()), err)]
    async fn transform(&self, query: Query) -> Result<Vec<Query>> {
        let mut variants = Vec::with_capacity(self.n_variants);
        for i in 0..self.n_variants {
            let req = EnrichRequest {
                text: query.text.clone(),
                intent: EnrichIntent::Custom(format!("rephrase_variant_{}", i)),
                context: None,
            };
            let enriched = self.enricher.enrich(req).await?;
            variants.push(Query {
                text: enriched.0,
                collection_id: query.collection_id.clone(),
                top_k: query.top_k,
                filters: query.filters.clone(),
            });
        }
        Ok(variants)
    }
}

// ---------------------------------------------------------------------------
// QueryRewrite — rewrite for clarity/correctness
// ---------------------------------------------------------------------------

/// Rewrites the query for improved retrieval recall.
/// Returns [rewritten_query].
pub struct QueryRewriteTransformer {
    enricher: Arc<dyn TextEnricher>,
}

impl QueryRewriteTransformer {
    pub fn new(enricher: Arc<dyn TextEnricher>) -> Self {
        Self { enricher }
    }
}

#[async_trait]
impl QueryTransformer for QueryRewriteTransformer {
    #[instrument(skip(self, query), fields(query_text_len = query.text.len()), err)]
    async fn transform(&self, query: Query) -> Result<Vec<Query>> {
        let req = EnrichRequest {
            text: query.text.clone(),
            intent: EnrichIntent::Custom("query_rewrite".to_string()),
            context: None,
        };
        let enriched = self.enricher.enrich(req).await?;
        Ok(vec![Query {
            text: enriched.0,
            collection_id: query.collection_id,
            top_k: query.top_k,
            filters: query.filters,
        }])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arcanum_core::types::EnrichedText;

    struct EchoEnricher;
    #[async_trait::async_trait]
    impl TextEnricher for EchoEnricher {
        async fn enrich(&self, req: EnrichRequest) -> arcanum_core::Result<EnrichedText> {
            Ok(EnrichedText(format!("[enriched] {}", req.text)))
        }
    }

    #[tokio::test]
    async fn test_hyde_returns_two_queries() {
        let t = HydeTransformer::new(Arc::new(EchoEnricher));
        let q = Query::new("what is RAG?");
        let result = t.transform(q).await.unwrap();
        assert_eq!(result.len(), 2, "HyDE should return exactly 2 queries");
        assert_eq!(result[0].text, "what is RAG?");
        assert!(result[1].text.contains("[enriched]"));
    }

    #[tokio::test]
    async fn test_multi_query_returns_n_variants() {
        let n = 4;
        let t = MultiQueryTransformer::new(Arc::new(EchoEnricher), n);
        let q = Query::new("what is RAG?");
        let result = t.transform(q).await.unwrap();
        assert_eq!(result.len(), n, "MultiQuery should return n variants");
    }

    #[tokio::test]
    async fn test_query_rewrite_returns_one_query() {
        let t = QueryRewriteTransformer::new(Arc::new(EchoEnricher));
        let q = Query::new("whats RAG?");
        let result = t.transform(q).await.unwrap();
        assert_eq!(result.len(), 1);
        assert!(result[0].text.contains("[enriched]"));
    }
}
