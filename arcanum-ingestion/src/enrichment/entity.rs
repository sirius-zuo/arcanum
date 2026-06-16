use arcanum_core::{traits::TextEnricher, types::*, Result};
use std::sync::Arc;
use serde::Deserialize;
use crate::sanitizer::sanitize_for_enrichment;
use tracing::instrument;

pub struct EntityExtractor {
    enricher: Arc<dyn TextEnricher>,
}

#[derive(Deserialize, Default)]
struct ExtractionResult {
    #[serde(default)]
    entities: Vec<ExtractedEntity>,
    #[serde(default)]
    relations: Vec<ExtractedRelation>,
}

#[derive(Deserialize)]
struct ExtractedEntity { name: String, entity_type: String }

#[derive(Deserialize)]
struct ExtractedRelation { source: String, relation: String, target: String }

impl EntityExtractor {
    pub fn new(enricher: Arc<dyn TextEnricher>) -> Self { Self { enricher } }

    #[instrument(skip(self, chunk), fields(chunk_id = ?chunk.id, entity_count, relation_count), err)]
    pub async fn extract(&self, chunk: &Chunk) -> Result<(Vec<Entity>, Vec<Relation>)> {
        let raw = self.enricher.enrich(EnrichRequest {
            text: sanitize_for_enrichment(&chunk.text),
            intent: EnrichIntent::ExtractEntities,
            context: None,
        }).await?;

        let parsed: ExtractionResult = serde_json::from_str(&raw.0)
            .unwrap_or_default();

        let source_uri = chunk.metadata.0.get("source_uri")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let mut entity_map = std::collections::HashMap::new();
        let entities: Vec<Entity> = parsed.entities.into_iter().map(|e| {
            let id = EntityId::new();
            entity_map.insert(e.name.clone(), id.clone());
            Entity { id, name: e.name, entity_type: e.entity_type,
                     canonical_id: None, source_chunks: vec![chunk.id.clone()],
                     source_uri: source_uri.clone(), collection_id: String::new() }
        }).collect();

        let relations: Vec<Relation> = parsed.relations.into_iter().filter_map(|r| {
            let src = entity_map.get(&r.source)?.clone();
            let tgt = entity_map.get(&r.target)?.clone();
            Some(Relation { source: src, relation_type: r.relation, target: tgt,
                            confidence: 0.9, source_chunks: vec![chunk.id.clone()] })
        }).collect();

        tracing::Span::current().record("entity_count", entities.len());
        tracing::Span::current().record("relation_count", relations.len());
        Ok((entities, relations))
    }
}
