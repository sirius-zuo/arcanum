use arcanum_core::{traits::*, types::*, Result};
use async_trait::async_trait;
use std::{collections::HashMap, sync::Arc};
use tokio::sync::RwLock;
use tracing::instrument;

pub mod query_planner;
pub use query_planner::GraphQueryPlanner;

pub mod neo4j_store;
pub use neo4j_store::Neo4jStore;

#[derive(Debug, Clone)]
pub struct GraphTraversalPlan {
    pub seed_entities: Vec<String>,
    pub max_hops: usize,
    pub relation_types: Vec<String>,
}

/// In-memory GraphStore for development and testing.
/// Replace with Kuzu or Neo4j implementation for production.
pub struct InMemoryGraphStore {
    entities: Arc<RwLock<HashMap<String, Entity>>>,
    relations: Arc<RwLock<Vec<Relation>>>,
}

impl InMemoryGraphStore {
    pub fn new() -> Self {
        Self {
            entities: Arc::new(RwLock::new(HashMap::new())),
            relations: Arc::new(RwLock::new(Vec::new())),
        }
    }
}

#[async_trait]
impl GraphStore for InMemoryGraphStore {
    #[instrument(skip(self, entities), fields(store = "in_memory_graph", entity_count = entities.len()), err)]
    async fn upsert_entities(&self, entities: Vec<Entity>) -> Result<()> {
        let mut map = self.entities.write().await;
        for e in entities { map.insert(e.id.0.to_string(), e); }
        Ok(())
    }

    #[instrument(skip(self, relations), fields(store = "in_memory_graph", relation_count = relations.len()), err)]
    async fn upsert_relations(&self, relations: Vec<Relation>) -> Result<()> {
        self.relations.write().await.extend(relations);
        Ok(())
    }

    #[instrument(skip(self, q), fields(store = "in_memory_graph", result_count), err)]
    async fn query(&self, q: &GraphQuery) -> Result<Vec<Entity>> {
        let map = self.entities.read().await;
        let results: Vec<Entity> = map.values().filter(|e| {
            q.entity_name.as_deref().map(|n| e.name.contains(n)).unwrap_or(true)
            && q.entity_type.as_deref().map(|t| e.entity_type == t).unwrap_or(true)
        }).cloned().collect();
        tracing::Span::current().record("result_count", results.len());
        Ok(results)
    }

    #[instrument(skip(self, entity_id), fields(store = "in_memory_graph", entity_id = %entity_id.0), err)]
    async fn get_relations(&self, entity_id: &EntityId) -> Result<Vec<Relation>> {
        Ok(self.relations.read().await.iter()
            .filter(|r| r.source.0 == entity_id.0)
            .cloned()
            .collect())
    }

    async fn delete_by_source_uri(&self, source_uri: &str) -> Result<()> {
        if source_uri.is_empty() {
            tracing::warn!(store = "in_memory_graph", "delete_by_source_uri called with empty source_uri — skipping to prevent mass deletion");
            return Ok(());
        }
        let mut entities = self.entities.write().await;
        let to_remove: std::collections::HashSet<String> = entities.values()
            .filter(|e| e.source_uri == source_uri)
            .map(|e| e.id.0.to_string())
            .collect();
        entities.retain(|id, _| !to_remove.contains(id));
        // Hold the entity lock while acquiring relation lock to prevent TOCTOU:
        let mut relations = self.relations.write().await;
        relations.retain(|r| {
            !to_remove.contains(&r.source.0.to_string())
            && !to_remove.contains(&r.target.0.to_string())
        });
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arcanum_core::traits::store::GraphQuery;

    #[tokio::test]
    async fn delete_by_source_uri_empty_string_is_noop() {
        let store = InMemoryGraphStore::new();
        let e = Entity {
            id: EntityId::new(), name: "Foo".into(), entity_type: "T".into(),
            canonical_id: None, source_chunks: vec![], source_uri: "".into(),
        };
        store.upsert_entities(vec![e]).await.unwrap();
        store.delete_by_source_uri("").await.unwrap();
        let results = store.query(&GraphQuery {
            entity_name: None, entity_type: None, max_hops: 1, relation_filter: None,
        }).await.unwrap();
        assert_eq!(results.len(), 1, "entity with source_uri='' must not be deleted by empty-string call");
    }

    #[tokio::test]
    async fn delete_by_source_uri_removes_entities_and_relations() {
        let store = InMemoryGraphStore::new();
        let id1 = EntityId::new();
        let id2 = EntityId::new();
        let e1 = Entity {
            id: id1.clone(), name: "Doc A".into(), entity_type: "Doc".into(),
            canonical_id: None, source_chunks: vec![], source_uri: "file://a.md".into(),
        };
        let e2 = Entity {
            id: id2.clone(), name: "Doc B".into(), entity_type: "Doc".into(),
            canonical_id: None, source_chunks: vec![], source_uri: "file://b.md".into(),
        };
        store.upsert_entities(vec![e1, e2]).await.unwrap();
        // Add a relation from e1 → e2 so we can verify cascade delete removes it
        let rel = arcanum_core::types::Relation {
            source: id1.clone(), relation_type: "links_to".into(), target: id2.clone(),
            confidence: 1.0, source_chunk: arcanum_core::types::ChunkId::new(),
        };
        store.upsert_relations(vec![rel]).await.unwrap();

        store.delete_by_source_uri("file://a.md").await.unwrap();

        let results = store.query(&GraphQuery {
            entity_name: None, entity_type: None, max_hops: 1, relation_filter: None,
        }).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "Doc B");

        // Verify the relation was also removed (cascade)
        let relations = store.get_relations(&id2).await.unwrap();
        assert!(relations.is_empty(), "relation from deleted entity should be removed");
    }
}
