use arcanum_core::{traits::GraphStore, traits::store::GraphQuery, types::*, ArcanumError, Result};
use async_trait::async_trait;
use neo4rs::{query, Graph};
use std::sync::Arc;
use tracing::instrument;

/// Neo4j-backed GraphStore for production deployments.
pub struct Neo4jStore {
    graph: Arc<Graph>,
}

impl Neo4jStore {
    pub async fn new(uri: &str, user: &str, password: &str) -> Result<Self> {
        let graph = Graph::new(uri, user, password).await
            .map_err(|e| ArcanumError::Config(format!("Neo4j connect error: {}", e)))?;
        Ok(Self { graph: Arc::new(graph) })
    }
}

#[async_trait]
impl GraphStore for Neo4jStore {
    #[instrument(skip(self, entities), fields(store = "neo4j", entity_count = entities.len()), err)]
    async fn upsert_entities(&self, entities: Vec<Entity>) -> Result<()> {
        for entity in entities {
            let id = entity.id.0.to_string();
            let name = entity.name.clone();
            let entity_type = entity.entity_type.clone();
            let canonical_id = entity.canonical_id.clone().unwrap_or_default();

            self.graph.run(
                query("MERGE (e:Entity {id: $id}) SET e.name = $name, e.entity_type = $entity_type, e.canonical_id = $canonical_id, e.source_uri = $source_uri")
                    .param("id", id)
                    .param("name", name)
                    .param("entity_type", entity_type)
                    .param("canonical_id", canonical_id)
                    .param("source_uri", entity.source_uri.clone()),
            )
            .await
            .map_err(|e| ArcanumError::Storage(format!("upsert_entity error: {}", e)))?;
        }
        Ok(())
    }

    #[instrument(skip(self, relations), fields(store = "neo4j", relation_count = relations.len()), err)]
    async fn upsert_relations(&self, relations: Vec<Relation>) -> Result<()> {
        for rel in relations {
            let source_id = rel.source.0.to_string();
            let target_id = rel.target.0.to_string();
            let relation_type = rel.relation_type.clone();
            let confidence = rel.confidence as f64;

            self.graph.run(
                query("MATCH (s:Entity {id: $source_id}) MATCH (t:Entity {id: $target_id}) MERGE (s)-[r:RELATION {type: $relation_type}]->(t) SET r.confidence = $confidence")
                    .param("source_id", source_id)
                    .param("target_id", target_id)
                    .param("relation_type", relation_type)
                    .param("confidence", confidence),
            )
            .await
            .map_err(|e| ArcanumError::Storage(format!("upsert_relation error: {}", e)))?;
        }
        Ok(())
    }

    #[instrument(skip(self, q), fields(store = "neo4j"), err)]
    async fn query(&self, q: &GraphQuery) -> Result<Vec<Entity>> {
        let name_pattern = q.entity_name.clone().unwrap_or_default();
        let entity_type = q.entity_type.clone().unwrap_or_default();

        let cypher = if !entity_type.is_empty() && !name_pattern.is_empty() {
            "MATCH (e:Entity) WHERE e.name CONTAINS $name AND e.entity_type = $entity_type RETURN e.id as id, e.name as name, e.entity_type as entity_type, e.canonical_id as canonical_id, e.source_uri as source_uri"
        } else if !entity_type.is_empty() {
            "MATCH (e:Entity) WHERE e.entity_type = $entity_type RETURN e.id as id, e.name as name, e.entity_type as entity_type, e.canonical_id as canonical_id, e.source_uri as source_uri"
        } else if !name_pattern.is_empty() {
            "MATCH (e:Entity) WHERE e.name CONTAINS $name RETURN e.id as id, e.name as name, e.entity_type as entity_type, e.canonical_id as canonical_id, e.source_uri as source_uri"
        } else {
            "MATCH (e:Entity) RETURN e.id as id, e.name as name, e.entity_type as entity_type, e.canonical_id as canonical_id, e.source_uri as source_uri LIMIT 100"
        };

        let mut stream = self.graph.execute(
            query(cypher)
                .param("name", name_pattern)
                .param("entity_type", entity_type),
        )
        .await
        .map_err(|e| ArcanumError::Storage(format!("query error: {}", e)))?;

        let mut entities = vec![];
        while let Some(row) = stream.next().await
            .map_err(|e| ArcanumError::Storage(format!("stream next error: {}", e)))? {
            let id_str: String = row.get("id")
                .map_err(|e| ArcanumError::Storage(format!("get id: {}", e)))?;
            let name: String = row.get("name")
                .map_err(|e| ArcanumError::Storage(format!("get name: {}", e)))?;
            let entity_type: String = row.get("entity_type")
                .map_err(|e| ArcanumError::Storage(format!("get entity_type: {}", e)))?;
            let canonical_id: Option<String> = row.get("canonical_id").ok();
            let source_uri: String = row.get("source_uri").unwrap_or_default();

            let id = id_str.parse::<uuid::Uuid>()
                .map_err(|e| ArcanumError::Storage(format!("parse uuid: {}", e)))?;

            entities.push(Entity {
                id: EntityId(id),
                name,
                entity_type,
                canonical_id: canonical_id.filter(|s| !s.is_empty()),
                source_chunks: vec![],
                source_uri,
            });
        }
        Ok(entities)
    }

    #[instrument(skip(self), fields(store = "neo4j", source_uri), err)]
    async fn delete_by_source_uri(&self, source_uri: &str) -> Result<()> {
        if source_uri.is_empty() {
            tracing::warn!(store = "neo4j", "delete_by_source_uri called with empty source_uri — skipping to prevent mass deletion");
            return Ok(());
        }
        self.graph.run(
            query("MATCH (e:Entity {source_uri: $source_uri}) DETACH DELETE e")
                .param("source_uri", source_uri.to_string()),
        )
        .await
        .map_err(|e| ArcanumError::Storage(format!("delete_by_source_uri error: {}", e)))?;
        Ok(())
    }

    #[instrument(skip(self, entity_id), fields(store = "neo4j", entity_id = %entity_id.0), err)]
    async fn get_relations(&self, entity_id: &EntityId) -> Result<Vec<Relation>> {
        let id_str = entity_id.0.to_string();

        let mut stream = self.graph.execute(
            query("MATCH (s:Entity {id: $id})-[r:RELATION]->(t:Entity) RETURN t.id as target_id, r.type as relation_type, r.confidence as confidence")
                .param("id", id_str),
        )
        .await
        .map_err(|e| ArcanumError::Storage(format!("get_relations error: {}", e)))?;

        let mut relations = vec![];
        while let Some(row) = stream.next().await
            .map_err(|e| ArcanumError::Storage(format!("stream next error: {}", e)))? {
            let target_id_str: String = row.get("target_id")
                .map_err(|e| ArcanumError::Storage(format!("get target_id: {}", e)))?;
            let relation_type: String = row.get("relation_type")
                .map_err(|e| ArcanumError::Storage(format!("get relation_type: {}", e)))?;
            let confidence: f64 = row.get("confidence").unwrap_or(1.0);

            let target_id = target_id_str.parse::<uuid::Uuid>()
                .map_err(|e| ArcanumError::Storage(format!("parse target uuid: {}", e)))?;

            let dummy_chunk_id = ChunkId::new();
            relations.push(Relation {
                source: EntityId(entity_id.0),
                relation_type,
                target: EntityId(target_id),
                confidence: confidence as f32,
                source_chunk: dummy_chunk_id,
            });
        }
        Ok(relations)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Smoke test — verifies the module compiles and types are correct.
    #[test]
    fn test_neo4j_store_module_compiles() {
        fn _assert_send_sync<T: Send + Sync>() {}
        _assert_send_sync::<Neo4jStore>();
    }

    /// Integration test — requires a live Neo4j instance.
    #[tokio::test]
    #[ignore]
    async fn test_neo4j_store_integration() {
        let uri = std::env::var("NEO4J_URI")
            .unwrap_or_else(|_| "bolt://localhost:7687".to_string());
        let user = std::env::var("NEO4J_USER").unwrap_or_else(|_| "neo4j".to_string());
        let password = std::env::var("NEO4J_PASSWORD").unwrap_or_else(|_| "password".to_string());

        let store = Neo4jStore::new(&uri, &user, &password).await.expect("connect");

        let entity = Entity {
            id: EntityId::new(),
            name: "Test Entity".to_string(),
            entity_type: "PERSON".to_string(),
            canonical_id: None,
            source_chunks: vec![],
            source_uri: "".to_string(),
        };
        store.upsert_entities(vec![entity.clone()]).await.expect("upsert");

        let q = GraphQuery {
            entity_name: Some("Test Entity".to_string()),
            entity_type: None,
            max_hops: 1,
            relation_filter: None,
        };
        let results = store.query(&q).await.expect("query");
        assert!(!results.is_empty());
    }
}
