use arcanum_core::{ArcanumError, traits::*, types::*, Result, traits::store::{relation_identity_key, relation_touches_removed_entity, merge_relation}};
use async_trait::async_trait;
use std::{collections::{HashMap, HashSet}, sync::Arc};
use tokio::sync::RwLock;
use tracing::instrument;

pub mod query_planner;
pub use query_planner::GraphQueryPlanner;

pub mod neo4j_store;
pub use neo4j_store::Neo4jStore;

pub mod sled_store;
pub use sled_store::SledGraphStore;

#[derive(Debug, Clone)]
pub struct GraphTraversalPlan {
    pub seed_entities: Vec<String>,
    pub max_hops: usize,
    pub relation_types: Vec<String>,
}

pub struct InMemoryGraphStore {
    // outer key: collection name; inner key: entity ID string
    entities:  Arc<RwLock<HashMap<String, HashMap<String, Entity>>>>,
    // key: relation_identity_key(source, relation_type, target) — global,
    // not collection-scoped (Neo4jStore's relation identity is global too;
    // its `collection` property is write-only).
    relations: Arc<RwLock<HashMap<Vec<u8>, Relation>>>,
    // tracks explicitly created collections (including empty ones)
    created:   Arc<RwLock<HashSet<String>>>,
}

impl InMemoryGraphStore {
    pub fn new() -> Self {
        Self {
            entities:  Arc::new(RwLock::new(HashMap::new())),
            relations: Arc::new(RwLock::new(HashMap::new())),
            created:   Arc::new(RwLock::new(HashSet::new())),
        }
    }
}

impl Default for InMemoryGraphStore {
    fn default() -> Self { Self::new() }
}

#[async_trait]
impl GraphStore for InMemoryGraphStore {
    #[instrument(skip(self, entities), fields(store = "in_memory_graph", collection, entity_count = entities.len()), err)]
    async fn upsert_entities(&self, collection: &str, entities: Vec<Entity>) -> Result<()> {
        let mut map = self.entities.write().await;
        let col = map.entry(collection.to_string()).or_default();
        for e in entities {
            col.insert(e.id.0.to_string(), e);
        }
        Ok(())
    }

    #[instrument(skip(self, relations), fields(store = "in_memory_graph", collection, relation_count = relations.len()), err)]
    async fn upsert_relations(&self, _collection: &str, relations: Vec<Relation>) -> Result<()> {
        let mut to_store = Vec::with_capacity(relations.len());
        for r in relations {
            let source_exists = self.get_entity_by_id(&r.source).await?.is_some();
            let target_exists = self.get_entity_by_id(&r.target).await?.is_some();
            if source_exists && target_exists {
                to_store.push(r);
            } else {
                tracing::warn!(
                    store = "in_memory_graph",
                    source_exists, target_exists,
                    "upsert_relations skipping relation with missing endpoint entity",
                );
            }
        }
        let mut map = self.relations.write().await;
        for r in to_store {
            let key = relation_identity_key(&r.source, &r.relation_type, &r.target);
            let merged = match map.remove(&key) {
                Some(existing) => merge_relation(existing, r),
                None => r,
            };
            map.insert(key, merged);
        }
        Ok(())
    }

    #[instrument(skip(self, q), fields(store = "in_memory_graph", collection, result_count), err)]
    async fn query(&self, collection: &str, q: &GraphQuery) -> Result<Vec<Entity>> {
        let map = self.entities.read().await;
        let entities = map.get(collection).cloned().unwrap_or_default();
        let results: Vec<Entity> = entities.values().filter(|e| {
            q.entity_name.as_deref().map(|n| e.name.contains(n)).unwrap_or(true)
            && q.entity_type.as_deref().map(|t| e.entity_type == t).unwrap_or(true)
        }).cloned().collect();
        tracing::Span::current().record("result_count", results.len());
        Ok(results)
    }

    #[instrument(skip(self, entity_id), fields(store = "in_memory_graph", entity_id = %entity_id.0), err)]
    async fn get_relations(&self, entity_id: &EntityId) -> Result<Vec<Relation>> {
        let all = self.relations.read().await;
        Ok(all.values()
            .filter(|r| r.source.0 == entity_id.0)
            .cloned()
            .collect())
    }

    #[instrument(skip(self), fields(store = "in_memory_graph", collection, source_uri), err)]
    async fn delete_by_source_uri(&self, collection: &str, source_uri: &str) -> Result<()> {
        if source_uri.is_empty() {
            tracing::warn!(store = "in_memory_graph", "delete_by_source_uri called with empty source_uri — skipping");
            return Ok(());
        }
        let mut entities = self.entities.write().await;
        let removed_ids: HashSet<String> = entities
            .get(collection)
            .map(|col| col.values()
                .filter(|e| e.source_uri == source_uri)
                .map(|e| e.id.0.to_string())
                .collect())
            .unwrap_or_default();

        if let Some(col) = entities.get_mut(collection) {
            col.retain(|id, _| !removed_ids.contains(id));
        }
        drop(entities);

        let mut relations = self.relations.write().await;
        relations.retain(|_, r| !relation_touches_removed_entity(&removed_ids, r));
        Ok(())
    }

    async fn list_collections(&self) -> Result<Vec<String>> {
        let entities = self.entities.read().await;
        let created = self.created.read().await;
        let mut all: HashSet<String> = entities.keys().cloned().collect();
        all.extend(created.iter().cloned());
        let mut result: Vec<String> = all.into_iter().collect();
        result.sort();
        Ok(result)
    }

    async fn create_collection(&self, collection: &str) -> Result<()> {
        let mut created = self.created.write().await;
        let entities = self.entities.read().await;
        if created.contains(collection) || entities.contains_key(collection) {
            return Err(ArcanumError::AlreadyExists(
                format!("collection '{}' already exists", collection),
            ));
        }
        created.insert(collection.to_string());
        Ok(())
    }

    async fn count_documents(&self, collection: Option<&str>) -> Result<u64> {
        let entities = self.entities.read().await;
        let count = match collection {
            Some(col) => {
                entities
                    .get(col)
                    .map(|m| {
                        m.values()
                            .map(|e| e.source_uri.as_str())
                            .filter(|u| !u.is_empty())
                            .collect::<HashSet<_>>()
                            .len()
                    })
                    .unwrap_or(0)
            }
            None => {
                entities
                    .values()
                    .flat_map(|m| m.values())
                    .map(|e| e.source_uri.as_str())
                    .filter(|u| !u.is_empty())
                    .collect::<HashSet<_>>()
                    .len()
            }
        };
        Ok(count as u64)
    }

    async fn count_documents_all(&self) -> Result<std::collections::HashMap<String, u64>> {
        let entities = self.entities.read().await;
        let created = self.created.read().await;
        let mut map: std::collections::HashMap<String, u64> = created
            .iter()
            .map(|c| (c.clone(), 0u64))
            .collect();
        for (col, entity_map) in entities.iter() {
            let count = entity_map.values()
                .map(|e| e.source_uri.as_str())
                .filter(|u| !u.is_empty())
                .collect::<HashSet<_>>()
                .len() as u64;
            map.insert(col.clone(), count);
        }
        Ok(map)
    }

    async fn delete_collection(&self, collection: &str) -> Result<()> {
        let mut entities = self.entities.write().await;
        let removed_ids: HashSet<String> = entities
            .get(collection)
            .map(|col| col.keys().cloned().collect())
            .unwrap_or_default();
        entities.remove(collection);
        drop(entities);

        let mut relations = self.relations.write().await;
        relations.retain(|_, r| !relation_touches_removed_entity(&removed_ids, r));
        drop(relations);

        self.created.write().await.remove(collection);
        Ok(())
    }

    async fn get_entity_by_id(&self, entity_id: &EntityId) -> Result<Option<Entity>> {
        let map = self.entities.read().await;
        Ok(map
            .values()
            .flat_map(|m| m.values())
            .find(|e| e.id.0 == entity_id.0)
            .cloned())
    }

    async fn get_relation(
        &self,
        source_id:     &EntityId,
        relation_type: &str,
        target_id:     &EntityId,
    ) -> Result<Option<Relation>> {
        let key = relation_identity_key(source_id, relation_type, target_id);
        Ok(self.relations.read().await.get(&key).cloned())
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
            canonical_id: None, source_chunks: vec![], source_uri: "".into(), collection_id: "test-col".into(),
        };
        store.upsert_entities("test-col", vec![e]).await.unwrap();
        store.delete_by_source_uri("test-col", "").await.unwrap();
        let results = store.query("test-col", &GraphQuery {
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
            canonical_id: None, source_chunks: vec![], source_uri: "file://a.md".into(), collection_id: "test-col".into(),
        };
        let e2 = Entity {
            id: id2.clone(), name: "Doc B".into(), entity_type: "Doc".into(),
            canonical_id: None, source_chunks: vec![], source_uri: "file://b.md".into(), collection_id: "test-col".into(),
        };
        store.upsert_entities("test-col", vec![e1, e2]).await.unwrap();
        // Add a relation from e1 → e2 so we can verify cascade delete removes it
        let rel = arcanum_core::types::Relation {
            source: id1.clone(), relation_type: "links_to".into(), target: id2.clone(),
            confidence: 1.0, source_chunks: vec![arcanum_core::types::ChunkId::new()],
        };
        store.upsert_relations("test-col", vec![rel]).await.unwrap();

        store.delete_by_source_uri("test-col", "file://a.md").await.unwrap();

        let results = store.query("test-col", &GraphQuery {
            entity_name: None, entity_type: None, max_hops: 1, relation_filter: None,
        }).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "Doc B");

        // Verify the relation was also removed (cascade)
        let relations = store.get_relations(&id2).await.unwrap();
        assert!(relations.is_empty(), "relation from deleted entity should be removed");
    }

    #[tokio::test]
    async fn collections_are_isolated() {
        let store = InMemoryGraphStore::new();
        let e1 = Entity {
            id: EntityId::new(), name: "Alpha".into(), entity_type: "T".into(),
            canonical_id: None, source_chunks: vec![],
            source_uri: "file://a.md".into(), collection_id: "col-a".into(),
        };
        let e2 = Entity {
            id: EntityId::new(), name: "Beta".into(), entity_type: "T".into(),
            canonical_id: None, source_chunks: vec![],
            source_uri: "file://b.md".into(), collection_id: "col-b".into(),
        };
        store.upsert_entities("col-a", vec![e1]).await.unwrap();
        store.upsert_entities("col-b", vec![e2]).await.unwrap();

        let gq = GraphQuery { entity_name: None, entity_type: None, max_hops: 1, relation_filter: None };
        let col_a = store.query("col-a", &gq).await.unwrap();
        assert_eq!(col_a.len(), 1);
        assert_eq!(col_a[0].name, "Alpha");

        let col_b = store.query("col-b", &gq).await.unwrap();
        assert_eq!(col_b.len(), 1);
        assert_eq!(col_b[0].name, "Beta");
    }

    #[tokio::test]
    async fn create_collection_and_list() {
        let store = InMemoryGraphStore::new();
        store.create_collection("empty-col").await.unwrap();
        let cols = store.list_collections().await.unwrap();
        assert!(cols.contains(&"empty-col".to_string()));
    }

    #[tokio::test]
    async fn create_collection_duplicate_returns_already_exists() {
        let store = InMemoryGraphStore::new();
        store.create_collection("col").await.unwrap();
        let err = store.create_collection("col").await.unwrap_err();
        assert!(matches!(err, arcanum_core::ArcanumError::AlreadyExists(_)));
    }

    #[tokio::test]
    async fn delete_collection_removes_entities_and_relations() {
        let store = InMemoryGraphStore::new();
        let id = EntityId::new();
        let e = Entity {
            id: id.clone(), name: "Foo".into(), entity_type: "T".into(),
            canonical_id: None, source_chunks: vec![],
            source_uri: "file://x.md".into(), collection_id: "col".into(),
        };
        store.upsert_entities("col", vec![e]).await.unwrap();
        store.delete_collection("col").await.unwrap();

        let gq = GraphQuery { entity_name: None, entity_type: None, max_hops: 1, relation_filter: None };
        let results = store.query("col", &gq).await.unwrap();
        assert!(results.is_empty());
        let cols = store.list_collections().await.unwrap();
        assert!(!cols.contains(&"col".to_string()));
    }

    #[tokio::test]
    async fn count_documents_by_source_uri() {
        let store = InMemoryGraphStore::new();
        // Two entities from the same document (same source_uri) — count should be 1.
        let e1 = Entity {
            id: EntityId::new(), name: "E1".into(), entity_type: "T".into(),
            canonical_id: None, source_chunks: vec![],
            source_uri: "file://doc.md".into(), collection_id: "col".into(),
        };
        let e2 = Entity {
            id: EntityId::new(), name: "E2".into(), entity_type: "T".into(),
            canonical_id: None, source_chunks: vec![],
            source_uri: "file://doc.md".into(), collection_id: "col".into(),
        };
        store.upsert_entities("col", vec![e1, e2]).await.unwrap();
        let count = store.count_documents(Some("col")).await.unwrap();
        assert_eq!(count, 1, "two entities from same doc = 1 document");

        let total = store.count_documents(None).await.unwrap();
        assert_eq!(total, 1);
    }

    #[tokio::test]
    async fn delete_by_source_uri_scoped_to_collection() {
        let store = InMemoryGraphStore::new();
        let e_a = Entity {
            id: EntityId::new(), name: "InA".into(), entity_type: "T".into(),
            canonical_id: None, source_chunks: vec![],
            source_uri: "file://shared.md".into(), collection_id: "col-a".into(),
        };
        let e_b = Entity {
            id: EntityId::new(), name: "InB".into(), entity_type: "T".into(),
            canonical_id: None, source_chunks: vec![],
            source_uri: "file://shared.md".into(), collection_id: "col-b".into(),
        };
        store.upsert_entities("col-a", vec![e_a]).await.unwrap();
        store.upsert_entities("col-b", vec![e_b]).await.unwrap();

        store.delete_by_source_uri("col-a", "file://shared.md").await.unwrap();

        let gq = GraphQuery { entity_name: None, entity_type: None, max_hops: 1, relation_filter: None };
        assert!(store.query("col-a", &gq).await.unwrap().is_empty());
        assert_eq!(store.query("col-b", &gq).await.unwrap().len(), 1, "col-b unaffected");
    }

    #[tokio::test]
    async fn count_documents_all_matches_per_collection_counts() {
        let store = InMemoryGraphStore::new();
        for (col, uri, n) in [("c1", "file://a.md", 2usize), ("c2", "file://b.md", 1)] {
            let entities: Vec<Entity> = (0..n).map(|_| Entity {
                id: EntityId::new(), name: "X".into(), entity_type: "T".into(),
                canonical_id: None, source_chunks: vec![],
                source_uri: uri.into(), collection_id: col.into(),
            }).collect();
            store.upsert_entities(col, entities).await.unwrap();
        }
        let all = store.count_documents_all().await.unwrap();
        assert_eq!(all.get("c1").copied().unwrap_or(0), 1, "c1 has 1 distinct source_uri");
        assert_eq!(all.get("c2").copied().unwrap_or(0), 1, "c2 has 1 distinct source_uri");
        // Total via count_documents(None) must match sum
        let total = store.count_documents(None).await.unwrap();
        assert_eq!(total, all.values().sum::<u64>());
    }

    #[tokio::test]
    async fn test_in_memory_get_entity_by_id() {
        let store = InMemoryGraphStore::new();
        let entity = Entity {
            id:           EntityId::new(),
            name:         "ACME Corp".into(),
            entity_type:  "Organization".into(),
            canonical_id: None,
            source_chunks: vec![],
            source_uri:   "file://contracts.pdf".into(),
            collection_id: "col".into(),
        };
        let id = entity.id.clone();
        store.upsert_entities("col", vec![entity]).await.unwrap();
        let found = store.get_entity_by_id(&id).await.unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "ACME Corp");
    }

    #[tokio::test]
    async fn test_in_memory_get_entity_by_id_missing() {
        let store = InMemoryGraphStore::new();
        let found = store.get_entity_by_id(&EntityId::new()).await.unwrap();
        assert!(found.is_none());
    }

    #[tokio::test]
    async fn test_in_memory_get_relation() {
        let store = InMemoryGraphStore::new();
        let src = EntityId::new();
        let tgt = EntityId::new();
        let e_src = Entity {
            id: src.clone(), name: "S".into(), entity_type: "T".into(),
            canonical_id: None, source_chunks: vec![], source_uri: "".into(), collection_id: "col".into(),
        };
        let e_tgt = Entity {
            id: tgt.clone(), name: "T".into(), entity_type: "T".into(),
            canonical_id: None, source_chunks: vec![], source_uri: "".into(), collection_id: "col".into(),
        };
        store.upsert_entities("col", vec![e_src, e_tgt]).await.unwrap();
        let rel = Relation {
            source:        src.clone(),
            relation_type: "SIGNED".into(),
            target:        tgt.clone(),
            confidence:    0.9,
            source_chunks: vec![],
        };
        store.upsert_relations("col", vec![rel]).await.unwrap();
        let found = store.get_relation(&src, "SIGNED", &tgt).await.unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().relation_type, "SIGNED");
    }

    #[tokio::test]
    async fn upsert_relations_merges_evidence_instead_of_overwriting() {
        let store = InMemoryGraphStore::new();
        let src = EntityId::new();
        let tgt = EntityId::new();
        let e_src = Entity {
            id: src.clone(), name: "S".into(), entity_type: "T".into(),
            canonical_id: None, source_chunks: vec![], source_uri: "".into(), collection_id: "col".into(),
        };
        let e_tgt = Entity {
            id: tgt.clone(), name: "T".into(), entity_type: "T".into(),
            canonical_id: None, source_chunks: vec![], source_uri: "".into(), collection_id: "col".into(),
        };
        store.upsert_entities("col", vec![e_src, e_tgt]).await.unwrap();
        let chunk_a = arcanum_core::types::ChunkId::new();
        let chunk_b = arcanum_core::types::ChunkId::new();
        let rel_v1 = Relation {
            source: src.clone(), relation_type: "SIGNED".into(), target: tgt.clone(),
            confidence: 0.5, source_chunks: vec![chunk_a.clone()],
        };
        let rel_v2 = Relation {
            source: src.clone(), relation_type: "SIGNED".into(), target: tgt.clone(),
            confidence: 0.9, source_chunks: vec![chunk_b.clone()],
        };
        store.upsert_relations("col", vec![rel_v1]).await.unwrap();
        store.upsert_relations("col", vec![rel_v2]).await.unwrap();

        let relations = store.get_relations(&src).await.unwrap();
        assert_eq!(relations.len(), 1, "re-upserting the same (source, type, target) must not duplicate");
        assert_eq!(relations[0].confidence, 0.9, "merge must keep the higher confidence");
        assert_eq!(relations[0].source_chunks.len(), 2, "merge must keep evidence from both upserts, not overwrite");
        assert!(relations[0].source_chunks.contains(&chunk_a));
        assert!(relations[0].source_chunks.contains(&chunk_b));
    }

    #[tokio::test]
    async fn delete_by_source_uri_cascades_when_deleted_entity_is_relation_target() {
        let store = InMemoryGraphStore::new();
        let id1 = EntityId::new();
        let id2 = EntityId::new();
        let e1 = Entity {
            id: id1.clone(), name: "Doc A".into(), entity_type: "Doc".into(),
            canonical_id: None, source_chunks: vec![], source_uri: "file://a.md".into(), collection_id: "test-col".into(),
        };
        let e2 = Entity {
            id: id2.clone(), name: "Doc B".into(), entity_type: "Doc".into(),
            canonical_id: None, source_chunks: vec![], source_uri: "file://b.md".into(), collection_id: "test-col".into(),
        };
        store.upsert_entities("test-col", vec![e1, e2]).await.unwrap();
        // Relation e1 -> e2: e2 is the target.
        let rel = arcanum_core::types::Relation {
            source: id1.clone(), relation_type: "links_to".into(), target: id2.clone(),
            confidence: 1.0, source_chunks: vec![],
        };
        store.upsert_relations("test-col", vec![rel]).await.unwrap();

        // Delete e2 (the target side) by its source_uri.
        store.delete_by_source_uri("test-col", "file://b.md").await.unwrap();

        let relations = store.get_relations(&id1).await.unwrap();
        assert!(relations.is_empty(), "relation must be cascade-deleted when its target entity is removed");
    }
}
