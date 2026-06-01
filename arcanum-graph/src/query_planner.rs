use arcanum_core::{
    traits::TextEnricher,
    types::{EnrichIntent, EnrichRequest},
    Result,
};
use crate::GraphTraversalPlan;
use async_trait::async_trait;
use std::sync::Arc;

pub struct GraphQueryPlanner {
    enricher: Arc<dyn TextEnricher>,
    default_max_hops: usize,
}

impl GraphQueryPlanner {
    pub fn new(enricher: Arc<dyn TextEnricher>, default_max_hops: usize) -> Self {
        Self { enricher, default_max_hops }
    }

    pub async fn plan(&self, query: &str) -> Result<GraphTraversalPlan> {
        if query.trim().is_empty() {
            return Ok(GraphTraversalPlan {
                seed_entities: vec![],
                max_hops: self.default_max_hops,
                relation_types: vec![],
            });
        }
        let req = EnrichRequest {
            text: query.to_string(),
            intent: EnrichIntent::ExtractEntities,
            context: None,
        };
        let enriched = self.enricher.enrich(req).await?;
        let seed_entities = parse_entity_names(&enriched.0);
        Ok(GraphTraversalPlan {
            seed_entities,
            max_hops: self.default_max_hops,
            relation_types: vec![],
        })
    }
}

#[async_trait]
impl arcanum_core::traits::GraphPlanner for GraphQueryPlanner {
    async fn plan_entities(&self, query: &str) -> arcanum_core::Result<Vec<String>> {
        let plan = self.plan(query).await?;
        Ok(plan.seed_entities)
    }
}

fn parse_entity_names(json_str: &str) -> Vec<String> {
    let val: serde_json::Value = serde_json::from_str(json_str).unwrap_or_default();
    val["entities"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|e| e["name"].as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use arcanum_core::types::{EnrichedText, EnrichRequest};

    struct FakeEntityExtractor;

    #[async_trait]
    impl arcanum_core::traits::TextEnricher for FakeEntityExtractor {
        async fn enrich(&self, _req: EnrichRequest) -> arcanum_core::Result<EnrichedText> {
            Ok(EnrichedText(
                r#"{"entities":[{"name":"Eiffel Tower","type":"LOCATION"}],"relations":[]}"#
                    .to_string(),
            ))
        }
    }

    #[tokio::test]
    async fn test_query_planner_extracts_entities() {
        let planner = GraphQueryPlanner::new(Arc::new(FakeEntityExtractor), 2);
        let plan = planner.plan("Where is the Eiffel Tower?").await.unwrap();
        assert!(!plan.seed_entities.is_empty());
        assert!(plan.seed_entities.iter().any(|e| e.contains("Eiffel")));
        assert_eq!(plan.max_hops, 2);
    }

    #[tokio::test]
    async fn test_query_planner_empty_query() {
        let planner = GraphQueryPlanner::new(Arc::new(FakeEntityExtractor), 3);
        let plan = planner.plan("   ").await.unwrap();
        assert!(plan.seed_entities.is_empty());
        assert_eq!(plan.max_hops, 3);
    }
}
