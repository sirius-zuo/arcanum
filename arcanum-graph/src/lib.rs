use arcanum_core::{traits::*, types::*, Result};
use async_trait::async_trait;
use std::{collections::HashMap, sync::Arc};
use tokio::sync::RwLock;

pub mod query_planner;
pub use query_planner::GraphQueryPlanner;

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
    async fn upsert_entities(&self, entities: Vec<Entity>) -> Result<()> {
        let mut map = self.entities.write().await;
        for e in entities { map.insert(e.id.0.to_string(), e); }
        Ok(())
    }

    async fn upsert_relations(&self, relations: Vec<Relation>) -> Result<()> {
        self.relations.write().await.extend(relations);
        Ok(())
    }

    async fn query(&self, q: &GraphQuery) -> Result<Vec<Entity>> {
        let map = self.entities.read().await;
        Ok(map.values().filter(|e| {
            q.entity_name.as_deref().map(|n| e.name.contains(n)).unwrap_or(true)
            && q.entity_type.as_deref().map(|t| e.entity_type == t).unwrap_or(true)
        }).cloned().collect())
    }

    async fn get_relations(&self, entity_id: &EntityId) -> Result<Vec<Relation>> {
        Ok(self.relations.read().await.iter()
            .filter(|r| r.source.0 == entity_id.0)
            .cloned()
            .collect())
    }
}
