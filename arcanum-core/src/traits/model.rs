use async_trait::async_trait;
use crate::types::{PerBackendChunkers, ShadowContext, *};
use crate::Result;

/// Implemented by the engine; called by each ingestion worker before running a task
/// to resolve per-collection chunkers and shadow context.
#[async_trait]
pub trait IngestionDepsOverrideResolver: Send + Sync {
    async fn resolve_for_collection(
        &self,
        collection_id: &str,
    ) -> Result<(PerBackendChunkers, Option<ShadowContext>)>;
}

#[async_trait]
pub trait TextEnricher: Send + Sync {
    async fn enrich(&self, request: EnrichRequest) -> Result<EnrichedText>;
}

#[async_trait]
pub trait Embedder: Send + Sync {
    async fn embed(&self, texts: Vec<String>) -> Result<Vec<Vector>>;
    fn dimension(&self) -> usize;
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockEnricher;
    #[async_trait::async_trait]
    impl TextEnricher for MockEnricher {
        async fn enrich(&self, req: EnrichRequest) -> crate::Result<EnrichedText> {
            Ok(EnrichedText(format!("[context] {}", req.text)))
        }
    }

    struct MockEmbedder;
    #[async_trait::async_trait]
    impl Embedder for MockEmbedder {
        async fn embed(&self, texts: Vec<String>) -> crate::Result<Vec<Vector>> {
            Ok(texts.iter().map(|_| Vector(vec![0.1, 0.2, 0.3])).collect())
        }
        fn dimension(&self) -> usize { 3 }
    }

    #[tokio::test]
    async fn test_enricher_context_prefix() {
        let e = MockEnricher;
        let result = e.enrich(EnrichRequest {
            text: "chunk text".to_string(),
            intent: EnrichIntent::ContextPrefix,
            context: None,
        }).await.unwrap();
        assert!(result.0.contains("chunk text"));
    }

    #[tokio::test]
    async fn test_embedder_dimension() {
        let e = MockEmbedder;
        let vecs = e.embed(vec!["a".into(), "b".into()]).await.unwrap();
        assert_eq!(vecs.len(), 2);
        assert_eq!(vecs[0].0.len(), e.dimension());
    }
}
