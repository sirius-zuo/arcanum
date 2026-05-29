use arcanum_core::{traits::TextEnricher, types::*, Result};
use std::sync::Arc;

pub struct ContextEnricher {
    enricher: Arc<dyn TextEnricher>,
}

impl ContextEnricher {
    pub fn new(enricher: Arc<dyn TextEnricher>) -> Self { Self { enricher } }

    pub async fn enrich_chunk(&self, mut chunk: Chunk, doc_context: &str) -> Result<Chunk> {
        let result = self.enricher.enrich(EnrichRequest {
            text: chunk.text.clone(),
            intent: EnrichIntent::ContextPrefix,
            context: Some(EnrichContext {
                document_title: Some(doc_context.to_string()),
                ..Default::default()
            }),
        }).await?;
        chunk.text = format!("{}\n{}", result.0, chunk.text);
        Ok(chunk)
    }
}
