use arcanum_models::EnrichmentDispatcher;
use arcanum_core::types::*;
use arcanum_core::traits::*;
use std::sync::Arc;

struct EchoEnricher;
#[async_trait::async_trait]
impl TextEnricher for EchoEnricher {
    async fn enrich(&self, req: EnrichRequest) -> arcanum_core::Result<EnrichedText> {
        Ok(EnrichedText(format!("echo:{}", req.text)))
    }
}

#[tokio::test]
async fn test_dispatcher_routes_by_intent() {
    let default = Arc::new(EchoEnricher) as Arc<dyn TextEnricher>;
    let entity  = Arc::new(EchoEnricher) as Arc<dyn TextEnricher>;
    let dispatcher = EnrichmentDispatcher::new(default)
        .with_override(EnrichIntent::ExtractEntities, entity);

    let result = dispatcher.enrich(EnrichRequest {
        text: "test".into(), intent: EnrichIntent::ExtractEntities, context: None,
    }).await.unwrap();
    assert!(result.0.starts_with("echo:"));
}
