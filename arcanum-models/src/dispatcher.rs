use arcanum_core::{traits::*, types::*, Result};
use async_trait::async_trait;
use std::{collections::HashMap, sync::Arc};
use tracing::instrument;

pub struct EnrichmentDispatcher {
    default: Arc<dyn TextEnricher>,
    overrides: HashMap<String, Arc<dyn TextEnricher>>,
}

impl EnrichmentDispatcher {
    pub fn new(default: Arc<dyn TextEnricher>) -> Self {
        Self {
            default,
            overrides: HashMap::new(),
        }
    }

    pub fn with_override(mut self, intent: EnrichIntent, provider: Arc<dyn TextEnricher>) -> Self {
        self.overrides.insert(intent_key(&intent), provider);
        self
    }
}

fn intent_key(intent: &EnrichIntent) -> String {
    match intent {
        EnrichIntent::ContextPrefix => "context_prefix".into(),
        EnrichIntent::Summarize => "summarize".into(),
        EnrichIntent::ExtractEntities => "extract_entities".into(),
        EnrichIntent::Caption => "caption".into(),
        EnrichIntent::Rerank => "rerank".into(),
        EnrichIntent::RewriteQuery => "rewrite_query".into(),
        EnrichIntent::Custom(s) => format!("custom:{}", s),
    }
}

#[async_trait]
impl TextEnricher for EnrichmentDispatcher {
    #[instrument(skip(self, request), fields(intent = ?request.intent), err)]
    async fn enrich(&self, request: EnrichRequest) -> Result<EnrichedText> {
        let key = intent_key(&request.intent);
        let provider = self.overrides.get(&key).unwrap_or(&self.default);
        provider.enrich(request).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixed(&'static str);

    #[async_trait]
    impl TextEnricher for Fixed {
        async fn enrich(&self, _request: EnrichRequest) -> Result<EnrichedText> {
            Ok(EnrichedText(self.0.into()))
        }
    }

    fn req(intent: EnrichIntent) -> EnrichRequest {
        EnrichRequest {
            text: "t".into(),
            intent,
            context: None,
        }
    }

    #[tokio::test]
    async fn rewrite_query_routes_to_its_override() {
        let d = EnrichmentDispatcher::new(Arc::new(Fixed("d")))
            .with_override(EnrichIntent::RewriteQuery, Arc::new(Fixed("r")));
        assert_eq!(
            d.enrich(req(EnrichIntent::RewriteQuery)).await.unwrap().0,
            "r"
        );
        assert_eq!(d.enrich(req(EnrichIntent::Summarize)).await.unwrap().0, "d");
    }
}
