use arcanum_ingestion::{ContextEnricher, EntityExtractor};
use arcanum_core::types::*;
use arcanum_core::traits::*;
use std::sync::Arc;

struct EchoEnricher;
#[async_trait::async_trait]
impl TextEnricher for EchoEnricher {
    async fn enrich(&self, req: EnrichRequest) -> arcanum_core::Result<EnrichedText> {
        Ok(EnrichedText(format!("[ctx] {}", req.text)))
    }
}

#[tokio::test]
async fn test_context_enricher_prepends_context() {
    let enricher = ContextEnricher::new(Arc::new(EchoEnricher));
    let chunk = Chunk {
        id: ChunkId::new(), text: "ownership rules".into(),
        document_id: DocumentId::new(),
        collection_id: CollectionId("test".into()),
        position: ChunkPosition { start: 0, end: 14, index: 0 },
        metadata: ChunkMetadata::default(),
    };
    let enriched = enricher.enrich_chunk(chunk, "Rust Book, Chapter 4").await.unwrap();
    assert!(enriched.text.contains("[ctx]"));
}
