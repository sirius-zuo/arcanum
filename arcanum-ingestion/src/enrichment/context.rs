use arcanum_core::{traits::TextEnricher, types::*, Result};
use std::sync::Arc;
use crate::sanitizer::sanitize_for_enrichment;
use tracing::instrument;

pub struct ContextEnricher {
    enricher: Arc<dyn TextEnricher>,
}

impl ContextEnricher {
    pub fn new(enricher: Arc<dyn TextEnricher>) -> Self { Self { enricher } }

    #[instrument(skip(self, chunk, doc_context), fields(chunk_id = ?chunk.id, doc_context_len = doc_context.len()), err)]
    pub async fn enrich_chunk(&self, mut chunk: Chunk, doc_context: &str) -> Result<Chunk> {
        let result = self.enricher.enrich(EnrichRequest {
            text: sanitize_for_enrichment(&chunk.text),
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
