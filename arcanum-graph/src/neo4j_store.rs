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
    #[instrument(skip(self, entities), fields(store = "neo4j", collection, entity_count = entities.len()), err)]
    async fn upsert_entities(&self, collection: &str, entities: Vec<Entity>) -> Result<()> {
        let futs: Vec<_> = entities
            .into_iter()
            .map(|entity| {
                let graph = Arc::clone(&self.graph);
                let col = collection.to_string();
                async move {
                    let id = entity.id.0.to_string();
                    graph
                        .run(
                            query(
                                "MERGE (e:Entity {id: $id}) \
                                 SET e.name = $name, e.entity_type = $entity_type, \
                                     e.canonical_id = $canonical_id, e.source_uri = $source_uri, \
                                     e.collection = $collection",
                            )
                            .param("id", id)
                            .param("name", entity.name)
                            .param("entity_type", entity.entity_type)
                            .param("canonical_id", entity.canonical_id.unwrap_or_default())
                            .param("source_uri", entity.source_uri)
                            .param("collection", col),
                        )
                        .await
                        .map_err(|e| ArcanumError::Storage(format!("upsert_entity error: {}", e)))
                }
            })
            .collect();
        futures::future::try_join_all(futs).await?;
        Ok(())
    }

    #[instrument(skip(self, relations), fields(store = "neo4j", collection, relation_count = relations.len()), err)]
    async fn upsert_relations(&self, collection: &str, relations: Vec<Relation>) -> Result<()> {
        let futs: Vec<_> = relations
            .into_iter()
            .map(|rel| {
                let graph = Arc::clone(&self.graph);
                let col = collection.to_string();
                async move {
                    graph
                        .run(
                            query(
                                "MATCH (s:Entity {id: $source_id}) \
                                 MATCH (t:Entity {id: $target_id}) \
                                 MERGE (s)-[r:RELATION {type: $relation_type}]->(t) \
                                 SET r.confidence = $confidence, r.collection = $collection",
                            )
                            .param("source_id", rel.source.0.to_string())
                            .param("target_id", rel.target.0.to_string())
                            .param("relation_type", rel.relation_type)
                            .param("confidence", rel.confidence as f64)
                            .param("collection", col),
                        )
                        .await
                        .map_err(|e| ArcanumError::Storage(format!("upsert_relation error: {}", e)))
                }
            })
            .collect();
        futures::future::try_join_all(futs).await?;
        Ok(())
    }

    #[instrument(skip(self, q), fields(store = "neo4j", collection), err)]
    async fn query(&self, collection: &str, q: &GraphQuery) -> Result<Vec<Entity>> {
        let name_pattern = q.entity_name.clone().unwrap_or_default();
        let entity_type = q.entity_type.clone().unwrap_or_default();

        let cypher = if !entity_type.is_empty() && !name_pattern.is_empty() {
            "MATCH (e:Entity {collection: $collection}) \
             WHERE e.name CONTAINS $name AND e.entity_type = $entity_type \
             RETURN e.id as id, e.name as name, e.entity_type as entity_type, \
                    e.canonical_id as canonical_id, e.source_uri as source_uri"
        } else if !entity_type.is_empty() {
            "MATCH (e:Entity {collection: $collection}) \
             WHERE e.entity_type = $entity_type \
             RETURN e.id as id, e.name as name, e.entity_type as entity_type, \
                    e.canonical_id as canonical_id, e.source_uri as source_uri"
        } else if !name_pattern.is_empty() {
            "MATCH (e:Entity {collection: $collection}) \
             WHERE e.name CONTAINS $name \
             RETURN e.id as id, e.name as name, e.entity_type as entity_type, \
                    e.canonical_id as canonical_id, e.source_uri as source_uri"
        } else {
            "MATCH (e:Entity {collection: $collection}) \
             RETURN e.id as id, e.name as name, e.entity_type as entity_type, \
                    e.canonical_id as canonical_id, e.source_uri as source_uri \
             LIMIT 100"
        };

        let mut stream = self.graph.execute(
            query(cypher)
                .param("collection", collection.to_string())
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
                collection_id: collection.to_string(),
            });
        }
        Ok(entities)
    }

    #[instrument(skip(self), fields(store = "neo4j", collection, source_uri), err)]
    async fn delete_by_source_uri(&self, collection: &str, source_uri: &str) -> Result<()> {
        if source_uri.is_empty() {
            tracing::warn!(store = "neo4j", "delete_by_source_uri called with empty source_uri — skipping");
            return Ok(());
        }
        self.graph.run(
            query("MATCH (e:Entity {collection: $collection, source_uri: $source_uri}) DETACH DELETE e")
                .param("collection", collection.to_string())
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

    async fn list_collections(&self) -> Result<Vec<String>> {
        let mut names: std::collections::HashSet<String> = std::collections::HashSet::new();

        // Source 1: explicitly created collections (GraphCollection metadata nodes)
        let mut meta_stream = self.graph.execute(
            query("MATCH (c:GraphCollection) RETURN c.name AS name"),
        )
        .await
        .map_err(|e| ArcanumError::Storage(format!("list_collections meta: {}", e)))?;
        while let Some(row) = meta_stream.next().await
            .map_err(|e| ArcanumError::Storage(format!("stream next: {}", e)))?
        {
            if let Ok(name) = row.get::<String>("name") {
                if !name.is_empty() { names.insert(name); }
            }
        }

        // Source 2: collections discovered from upserted entities (pipeline path)
        let mut entity_stream = self.graph.execute(
            query("MATCH (e:Entity) WHERE e.collection IS NOT NULL \
                   RETURN DISTINCT e.collection AS name"),
        )
        .await
        .map_err(|e| ArcanumError::Storage(format!("list_collections entities: {}", e)))?;
        while let Some(row) = entity_stream.next().await
            .map_err(|e| ArcanumError::Storage(format!("stream next: {}", e)))? {
            if let Ok(name) = row.get::<String>("name") {
                if !name.is_empty() { names.insert(name); }
            }
        }

        let mut result: Vec<String> = names.into_iter().collect();
        result.sort();
        Ok(result)
    }

    async fn create_collection(&self, collection: &str) -> Result<()> {
        let mut stream = self.graph.execute(
            query(
                "MERGE (c:GraphCollection {name: $name}) \
                 ON CREATE SET c.just_created = true \
                 ON MATCH  SET c.just_created = false \
                 RETURN c.just_created AS just_created",
            )
            .param("name", collection.to_string()),
        )
        .await
        .map_err(|e| ArcanumError::Storage(format!("create_collection error: {}", e)))?;

        let just_created: bool = if let Some(row) = stream.next().await
            .map_err(|e| ArcanumError::Storage(format!("stream next: {}", e)))?
        {
            row.get("just_created").unwrap_or(false)
        } else {
            false
        };

        if !just_created {
            return Err(ArcanumError::AlreadyExists(
                format!("collection '{}' already exists", collection),
            ));
        }
        Ok(())
    }

    async fn count_documents(&self, collection: Option<&str>) -> Result<u64> {
        let (cypher, col_param) = match collection {
            Some(col) => (
                "MATCH (e:Entity {collection: $collection}) \
                 RETURN COUNT(DISTINCT e.source_uri) as cnt",
                col.to_string(),
            ),
            None => (
                "MATCH (e:Entity) RETURN COUNT(DISTINCT e.source_uri) as cnt",
                String::new(),
            ),
        };
        let mut q = query(cypher);
        if collection.is_some() {
            q = q.param("collection", col_param);
        }
        let mut stream = self.graph.execute(q)
            .await
            .map_err(|e| ArcanumError::Storage(format!("count_documents error: {}", e)))?;
        let count: i64 = if let Some(row) = stream.next().await
            .map_err(|e| ArcanumError::Storage(format!("stream next: {}", e)))? {
            row.get("cnt").unwrap_or(0)
        } else {
            0
        };
        Ok(count as u64)
    }

    async fn count_documents_all(&self) -> Result<std::collections::HashMap<String, u64>> {
        // Seed with all known collections (including empty ones) so they appear with count 0.
        let mut map: std::collections::HashMap<String, u64> = self
            .list_collections()
            .await?
            .into_iter()
            .map(|c| (c, 0u64))
            .collect();

        // One aggregated query returns counts for all collections with data.
        let mut stream = self.graph.execute(
            query(
                "MATCH (e:Entity) \
                 WHERE e.collection IS NOT NULL \
                   AND e.source_uri IS NOT NULL AND e.source_uri <> '' \
                 RETURN e.collection AS col, COUNT(DISTINCT e.source_uri) AS cnt",
            ),
        )
        .await
        .map_err(|e| ArcanumError::Storage(format!("count_documents_all error: {}", e)))?;

        while let Some(row) = stream.next().await
            .map_err(|e| ArcanumError::Storage(format!("stream next: {}", e)))?
        {
            let col: String = row.get("col")
                .map_err(|e| ArcanumError::Storage(format!("get col: {}", e)))?;
            let cnt: i64 = row.get("cnt").unwrap_or(0);
            map.entry(col).and_modify(|v| *v = cnt as u64).or_insert(cnt as u64);
        }
        Ok(map)
    }

    async fn delete_collection(&self, collection: &str) -> Result<()> {
        // Delete all entities in the collection (DETACH DELETE cascades relations).
        self.graph.run(
            query("MATCH (e:Entity {collection: $collection}) DETACH DELETE e")
                .param("collection", collection.to_string()),
        )
        .await
        .map_err(|e| ArcanumError::Storage(format!("delete_collection entities: {}", e)))?;
        // Remove the collection metadata node.
        self.graph.run(
            query("MATCH (c:GraphCollection {name: $name}) DELETE c")
                .param("name", collection.to_string()),
        )
        .await
        .map_err(|e| ArcanumError::Storage(format!("delete_collection metadata: {}", e)))?;
        Ok(())
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
            collection_id: "test-col".to_string(),
        };
        store.upsert_entities("test-col", vec![entity.clone()]).await.expect("upsert");

        let q = GraphQuery {
            entity_name: Some("Test Entity".to_string()),
            entity_type: None,
            max_hops: 1,
            relation_filter: None,
        };
        let results = store.query("test-col", &q).await.expect("query");
        assert!(!results.is_empty());
    }

    /// Verifies that list_collections returns collections populated by upsert_entities
    /// even without a prior create_collection call.
    /// Run with: cargo test -p arcanum-graph -- test_list_collections_includes_entity_collections --include-ignored
    #[tokio::test]
    #[ignore]
    async fn test_list_collections_includes_entity_collections() {
        let uri = std::env::var("NEO4J_URI").unwrap_or_else(|_| "bolt://localhost:7687".to_string());
        let user = std::env::var("NEO4J_USER").unwrap_or_else(|_| "neo4j".to_string());
        let password = std::env::var("NEO4J_PASSWORD").unwrap_or_else(|_| "password".to_string());
        let store = Neo4jStore::new(&uri, &user, &password).await.expect("connect");

        let col = format!("test-entity-col-{}", uuid::Uuid::new_v4());
        let entity = Entity {
            id: EntityId::new(), name: "Pipeline Entity".into(),
            entity_type: "T".into(), canonical_id: None, source_chunks: vec![],
            source_uri: "file://doc.md".into(), collection_id: col.clone(),
        };
        // Intentionally NOT calling create_collection first:
        store.upsert_entities(&col, vec![entity]).await.expect("upsert");

        let cols = store.list_collections().await.expect("list");
        assert!(cols.contains(&col),
            "list_collections must include collections populated by upsert_entities");

        // Cleanup
        store.delete_collection(&col).await.ok();
    }
}
