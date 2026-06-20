use arcanum_core::{traits::GraphStore, traits::store::GraphQuery, types::*, ArcanumError, Result};
use async_trait::async_trait;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use tokio::sync::Mutex;
use tracing::instrument;

fn entity_key(collection: &str, entity_id: &str) -> Vec<u8> {
    format!("{collection}\0{entity_id}").into_bytes()
}

fn entity_prefix(collection: &str) -> Vec<u8> {
    format!("{collection}\0").into_bytes()
}

fn relation_key(source: &EntityId, relation_type: &str, target: &EntityId) -> Vec<u8> {
    format!("{}\0{}\0{}", source.0, relation_type, target.0).into_bytes()
}

/// Sled-backed GraphStore for persistent local dev/test use — no server process required.
/// Relations are stored globally (not collection-partitioned), matching Neo4jStore's real
/// MERGE semantics; entities stay collection-partitioned, matching Neo4jStore's e.collection filter.
pub struct SledGraphStore {
    db:          sled::Db,
    entities:    sled::Tree,
    relations:   sled::Tree,
    collections: sled::Tree,
    write_lock:  Mutex<()>,
}

impl SledGraphStore {
    pub fn new(path: impl AsRef<Path>) -> Result<Self> {
        let db = sled::open(path.as_ref())
            .map_err(|e| ArcanumError::Storage(format!("open sled db: {e}")))?;
        let entities = db.open_tree("entities")
            .map_err(|e| ArcanumError::Storage(format!("open entities tree: {e}")))?;
        let relations = db.open_tree("relations")
            .map_err(|e| ArcanumError::Storage(format!("open relations tree: {e}")))?;
        let collections = db.open_tree("collections")
            .map_err(|e| ArcanumError::Storage(format!("open collections tree: {e}")))?;
        Ok(Self { db, entities, relations, collections, write_lock: Mutex::new(()) })
    }

    fn cascade_delete_relations(&self, removed_ids: &HashSet<String>) -> Result<()> {
        if removed_ids.is_empty() {
            return Ok(());
        }
        let items: Vec<(sled::IVec, sled::IVec)> = self.relations.iter()
            .collect::<sled::Result<Vec<_>>>()
            .map_err(|e| ArcanumError::Storage(format!("scan relations: {e}")))?;
        for (key, value) in items {
            let relation: Relation = serde_json::from_slice(&value)
                .map_err(|e| ArcanumError::Storage(format!("deserialize relation: {e}")))?;
            if removed_ids.contains(&relation.source.0.to_string())
                || removed_ids.contains(&relation.target.0.to_string())
            {
                self.relations.remove(&key)
                    .map_err(|e| ArcanumError::Storage(format!("remove relation: {e}")))?;
            }
        }
        Ok(())
    }
}

#[async_trait]
impl GraphStore for SledGraphStore {
    #[instrument(skip(self, entities), fields(store = "sled_graph", collection, entity_count = entities.len()), err)]
    async fn upsert_entities(&self, collection: &str, entities: Vec<Entity>) -> Result<()> {
        let _guard = self.write_lock.lock().await;
        for e in &entities {
            let key = entity_key(collection, &e.id.0.to_string());
            let value = serde_json::to_vec(e)
                .map_err(|err| ArcanumError::Storage(format!("serialize entity: {err}")))?;
            self.entities.insert(key, value)
                .map_err(|err| ArcanumError::Storage(format!("insert entity: {err}")))?;
        }
        self.collections.insert(collection.as_bytes(), Vec::<u8>::new())
            .map_err(|err| ArcanumError::Storage(format!("mark collection: {err}")))?;
        self.db.flush_async().await
            .map_err(|err| ArcanumError::Storage(format!("flush: {err}")))?;
        Ok(())
    }

    #[instrument(skip(self, relations), fields(store = "sled_graph", collection, relation_count = relations.len()), err)]
    async fn upsert_relations(&self, _collection: &str, relations: Vec<Relation>) -> Result<()> {
        let _guard = self.write_lock.lock().await;
        for r in &relations {
            let key = relation_key(&r.source, &r.relation_type, &r.target);
            let value = serde_json::to_vec(r)
                .map_err(|e| ArcanumError::Storage(format!("serialize relation: {e}")))?;
            self.relations.insert(key, value)
                .map_err(|e| ArcanumError::Storage(format!("insert relation: {e}")))?;
        }
        self.db.flush_async().await
            .map_err(|e| ArcanumError::Storage(format!("flush: {e}")))?;
        Ok(())
    }

    #[instrument(skip(self, q), fields(store = "sled_graph", collection, result_count), err)]
    async fn query(&self, collection: &str, q: &GraphQuery) -> Result<Vec<Entity>> {
        let prefix = entity_prefix(collection);
        let mut results = vec![];
        for item in self.entities.scan_prefix(&prefix) {
            let (_, value) = item.map_err(|e| ArcanumError::Storage(format!("scan entities: {e}")))?;
            let entity: Entity = serde_json::from_slice(&value)
                .map_err(|e| ArcanumError::Storage(format!("deserialize entity: {e}")))?;
            let name_ok = q.entity_name.as_deref().map(|n| entity.name.contains(n)).unwrap_or(true);
            let type_ok = q.entity_type.as_deref().map(|t| entity.entity_type == t).unwrap_or(true);
            if name_ok && type_ok {
                results.push(entity);
            }
        }
        tracing::Span::current().record("result_count", results.len());
        Ok(results)
    }

    async fn get_relations(&self, entity_id: &EntityId) -> Result<Vec<Relation>> {
        let mut results = vec![];
        for item in self.relations.iter() {
            let (_, value) = item.map_err(|e| ArcanumError::Storage(format!("scan relations: {e}")))?;
            let relation: Relation = serde_json::from_slice(&value)
                .map_err(|e| ArcanumError::Storage(format!("deserialize relation: {e}")))?;
            if relation.source.0 == entity_id.0 {
                results.push(relation);
            }
        }
        Ok(results)
    }

    #[instrument(skip(self), fields(store = "sled_graph", collection, source_uri), err)]
    async fn delete_by_source_uri(&self, collection: &str, source_uri: &str) -> Result<()> {
        if source_uri.is_empty() {
            tracing::warn!(store = "sled_graph", "delete_by_source_uri called with empty source_uri — skipping");
            return Ok(());
        }
        let _guard = self.write_lock.lock().await;
        let items: Vec<(sled::IVec, sled::IVec)> = self.entities.scan_prefix(entity_prefix(collection))
            .collect::<sled::Result<Vec<_>>>()
            .map_err(|e| ArcanumError::Storage(format!("scan entities: {e}")))?;
        let mut removed_ids: HashSet<String> = HashSet::new();
        for (key, value) in items {
            let entity: Entity = serde_json::from_slice(&value)
                .map_err(|e| ArcanumError::Storage(format!("deserialize entity: {e}")))?;
            if entity.source_uri == source_uri {
                removed_ids.insert(entity.id.0.to_string());
                self.entities.remove(&key)
                    .map_err(|e| ArcanumError::Storage(format!("remove entity: {e}")))?;
            }
        }
        self.cascade_delete_relations(&removed_ids)?;
        self.db.flush_async().await
            .map_err(|e| ArcanumError::Storage(format!("flush: {e}")))?;
        Ok(())
    }

    async fn list_collections(&self) -> Result<Vec<String>> {
        let mut names: HashSet<String> = HashSet::new();
        for item in self.collections.iter() {
            let (key, _) = item.map_err(|e| ArcanumError::Storage(format!("scan collections: {e}")))?;
            names.insert(String::from_utf8_lossy(&key).to_string());
        }
        for item in self.entities.iter() {
            let (key, _) = item.map_err(|e| ArcanumError::Storage(format!("scan entities: {e}")))?;
            let key_str = String::from_utf8_lossy(&key);
            if let Some(idx) = key_str.find('\0') {
                names.insert(key_str[..idx].to_string());
            }
        }
        let mut result: Vec<String> = names.into_iter().collect();
        result.sort();
        Ok(result)
    }

    async fn create_collection(&self, collection: &str) -> Result<()> {
        let _guard = self.write_lock.lock().await;
        let already_marked = self.collections.contains_key(collection.as_bytes())
            .map_err(|e| ArcanumError::Storage(format!("check collection: {e}")))?;
        let already_has_entities = self.entities.scan_prefix(entity_prefix(collection)).next().is_some();
        if already_marked || already_has_entities {
            return Err(ArcanumError::AlreadyExists(
                format!("collection '{}' already exists", collection),
            ));
        }
        self.collections.insert(collection.as_bytes(), Vec::<u8>::new())
            .map_err(|e| ArcanumError::Storage(format!("create collection: {e}")))?;
        self.db.flush_async().await
            .map_err(|e| ArcanumError::Storage(format!("flush: {e}")))?;
        Ok(())
    }

    async fn count_documents(&self, collection: Option<&str>) -> Result<u64> {
        let items: Vec<(sled::IVec, sled::IVec)> = match collection {
            Some(col) => self.entities.scan_prefix(entity_prefix(col))
                .collect::<sled::Result<Vec<_>>>()
                .map_err(|e| ArcanumError::Storage(format!("scan entities: {e}")))?,
            None => self.entities.iter()
                .collect::<sled::Result<Vec<_>>>()
                .map_err(|e| ArcanumError::Storage(format!("scan entities: {e}")))?,
        };
        let mut uris: HashSet<String> = HashSet::new();
        for (_, value) in items {
            let entity: Entity = serde_json::from_slice(&value)
                .map_err(|e| ArcanumError::Storage(format!("deserialize entity: {e}")))?;
            if !entity.source_uri.is_empty() {
                uris.insert(entity.source_uri);
            }
        }
        Ok(uris.len() as u64)
    }

    async fn count_documents_all(&self) -> Result<HashMap<String, u64>> {
        let mut map: HashMap<String, u64> = HashMap::new();
        for item in self.collections.iter() {
            let (key, _) = item.map_err(|e| ArcanumError::Storage(format!("scan collections: {e}")))?;
            map.insert(String::from_utf8_lossy(&key).to_string(), 0);
        }
        let mut per_collection_uris: HashMap<String, HashSet<String>> = HashMap::new();
        for item in self.entities.iter() {
            let (key, value) = item.map_err(|e| ArcanumError::Storage(format!("scan entities: {e}")))?;
            let key_str = String::from_utf8_lossy(&key);
            let collection = match key_str.find('\0') {
                Some(idx) => key_str[..idx].to_string(),
                None => continue,
            };
            let entity: Entity = serde_json::from_slice(&value)
                .map_err(|e| ArcanumError::Storage(format!("deserialize entity: {e}")))?;
            if !entity.source_uri.is_empty() {
                per_collection_uris.entry(collection).or_default().insert(entity.source_uri);
            }
        }
        for (col, uris) in per_collection_uris {
            map.insert(col, uris.len() as u64);
        }
        Ok(map)
    }

    async fn delete_collection(&self, collection: &str) -> Result<()> {
        let _guard = self.write_lock.lock().await;
        let items: Vec<(sled::IVec, sled::IVec)> = self.entities.scan_prefix(entity_prefix(collection))
            .collect::<sled::Result<Vec<_>>>()
            .map_err(|e| ArcanumError::Storage(format!("scan entities: {e}")))?;
        let mut removed_ids: HashSet<String> = HashSet::new();
        for (key, value) in items {
            let entity: Entity = serde_json::from_slice(&value)
                .map_err(|e| ArcanumError::Storage(format!("deserialize entity: {e}")))?;
            removed_ids.insert(entity.id.0.to_string());
            self.entities.remove(&key)
                .map_err(|e| ArcanumError::Storage(format!("remove entity: {e}")))?;
        }
        self.collections.remove(collection.as_bytes())
            .map_err(|e| ArcanumError::Storage(format!("remove collection marker: {e}")))?;
        self.cascade_delete_relations(&removed_ids)?;
        self.db.flush_async().await
            .map_err(|e| ArcanumError::Storage(format!("flush: {e}")))?;
        Ok(())
    }

    async fn get_entity_by_id(&self, entity_id: &EntityId) -> Result<Option<Entity>> {
        for item in self.entities.iter() {
            let (_, value) = item.map_err(|e| ArcanumError::Storage(format!("scan entities: {e}")))?;
            let entity: Entity = serde_json::from_slice(&value)
                .map_err(|e| ArcanumError::Storage(format!("deserialize entity: {e}")))?;
            if entity.id.0 == entity_id.0 {
                return Ok(Some(entity));
            }
        }
        Ok(None)
    }

    async fn get_relation(
        &self,
        source_id:     &EntityId,
        relation_type: &str,
        target_id:     &EntityId,
    ) -> Result<Option<Relation>> {
        let key = relation_key(source_id, relation_type, target_id);
        match self.relations.get(&key)
            .map_err(|e| ArcanumError::Storage(format!("get relation: {e}")))?
        {
            Some(bytes) => {
                let relation = serde_json::from_slice(&bytes)
                    .map_err(|e| ArcanumError::Storage(format!("deserialize relation: {e}")))?;
                Ok(Some(relation))
            }
            None => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_store() -> (SledGraphStore, TempDir) {
        let tmp = TempDir::new().unwrap();
        let store = SledGraphStore::new(tmp.path()).unwrap();
        (store, tmp)
    }

    #[tokio::test]
    async fn upsert_and_query_entity_by_name_substring() {
        let (store, _tmp) = make_store();
        let entity = Entity {
            id: EntityId::new(), name: "ACME Corp".into(), entity_type: "Organization".into(),
            canonical_id: None, source_chunks: vec![], source_uri: "file://contracts.pdf".into(),
            collection_id: "col".into(),
        };
        store.upsert_entities("col", vec![entity]).await.unwrap();

        let results = store.query("col", &GraphQuery {
            entity_name: Some("ACME".into()), entity_type: None, max_hops: 1, relation_filter: None,
        }).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "ACME Corp");
    }

    #[tokio::test]
    async fn upsert_entity_overwrites_by_id() {
        let (store, _tmp) = make_store();
        let id = EntityId::new();
        let v1 = Entity {
            id: id.clone(), name: "Old Name".into(), entity_type: "T".into(),
            canonical_id: None, source_chunks: vec![], source_uri: "".into(), collection_id: "col".into(),
        };
        let v2 = Entity {
            id: id.clone(), name: "New Name".into(), entity_type: "T".into(),
            canonical_id: None, source_chunks: vec![], source_uri: "".into(), collection_id: "col".into(),
        };
        store.upsert_entities("col", vec![v1]).await.unwrap();
        store.upsert_entities("col", vec![v2]).await.unwrap();

        let found = store.get_entity_by_id(&id).await.unwrap().unwrap();
        assert_eq!(found.name, "New Name");
    }

    #[tokio::test]
    async fn collections_are_isolated() {
        let (store, _tmp) = make_store();
        let e1 = Entity {
            id: EntityId::new(), name: "Alpha".into(), entity_type: "T".into(),
            canonical_id: None, source_chunks: vec![], source_uri: "file://a.md".into(), collection_id: "col-a".into(),
        };
        let e2 = Entity {
            id: EntityId::new(), name: "Beta".into(), entity_type: "T".into(),
            canonical_id: None, source_chunks: vec![], source_uri: "file://b.md".into(), collection_id: "col-b".into(),
        };
        store.upsert_entities("col-a", vec![e1]).await.unwrap();
        store.upsert_entities("col-b", vec![e2]).await.unwrap();

        let gq = GraphQuery { entity_name: None, entity_type: None, max_hops: 1, relation_filter: None };
        let col_a = store.query("col-a", &gq).await.unwrap();
        assert_eq!(col_a.len(), 1);
        assert_eq!(col_a[0].name, "Alpha");
    }

    #[tokio::test]
    async fn upsert_relations_is_idempotent_by_source_type_target() {
        let (store, _tmp) = make_store();
        let src = EntityId::new();
        let tgt = EntityId::new();
        let rel_v1 = Relation {
            source: src.clone(), relation_type: "SIGNED".into(), target: tgt.clone(),
            confidence: 0.5, source_chunks: vec![],
        };
        let rel_v2 = Relation {
            source: src.clone(), relation_type: "SIGNED".into(), target: tgt.clone(),
            confidence: 0.9, source_chunks: vec![],
        };
        store.upsert_relations("col", vec![rel_v1]).await.unwrap();
        store.upsert_relations("col", vec![rel_v2]).await.unwrap();

        let relations = store.get_relations(&src).await.unwrap();
        assert_eq!(relations.len(), 1);
        assert_eq!(relations[0].confidence, 0.9);
    }

    #[tokio::test]
    async fn get_relation_looks_up_by_source_type_target() {
        let (store, _tmp) = make_store();
        let src = EntityId::new();
        let tgt = EntityId::new();
        let rel = Relation {
            source: src.clone(), relation_type: "WORKS_AT".into(), target: tgt.clone(),
            confidence: 0.9, source_chunks: vec![],
        };
        store.upsert_relations("col", vec![rel]).await.unwrap();

        let found = store.get_relation(&src, "WORKS_AT", &tgt).await.unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().relation_type, "WORKS_AT");

        let missing = store.get_relation(&src, "OTHER_TYPE", &tgt).await.unwrap();
        assert!(missing.is_none());
    }

    #[tokio::test]
    async fn delete_by_source_uri_empty_string_is_noop() {
        let (store, _tmp) = make_store();
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
        let (store, _tmp) = make_store();
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
        let rel = Relation {
            source: id1.clone(), relation_type: "links_to".into(), target: id2.clone(),
            confidence: 1.0, source_chunks: vec![],
        };
        store.upsert_relations("test-col", vec![rel]).await.unwrap();

        store.delete_by_source_uri("test-col", "file://a.md").await.unwrap();

        let results = store.query("test-col", &GraphQuery {
            entity_name: None, entity_type: None, max_hops: 1, relation_filter: None,
        }).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "Doc B");

        let relations = store.get_relations(&id1).await.unwrap();
        assert!(relations.is_empty(), "relation from deleted source entity must be cascade-removed");
    }

    #[tokio::test]
    async fn delete_by_source_uri_cascades_when_deleted_entity_is_relation_target() {
        let (store, _tmp) = make_store();
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
        let rel = Relation {
            source: id1.clone(), relation_type: "links_to".into(), target: id2.clone(),
            confidence: 1.0, source_chunks: vec![],
        };
        store.upsert_relations("test-col", vec![rel]).await.unwrap();

        // Delete e2, the relation's target.
        store.delete_by_source_uri("test-col", "file://b.md").await.unwrap();

        let relations = store.get_relations(&id1).await.unwrap();
        assert!(relations.is_empty(), "relation must be cascade-deleted when its target entity is removed");
    }

    #[tokio::test]
    async fn delete_by_source_uri_scoped_to_collection() {
        let (store, _tmp) = make_store();
        let e_a = Entity {
            id: EntityId::new(), name: "InA".into(), entity_type: "T".into(),
            canonical_id: None, source_chunks: vec![], source_uri: "file://shared.md".into(), collection_id: "col-a".into(),
        };
        let e_b = Entity {
            id: EntityId::new(), name: "InB".into(), entity_type: "T".into(),
            canonical_id: None, source_chunks: vec![], source_uri: "file://shared.md".into(), collection_id: "col-b".into(),
        };
        store.upsert_entities("col-a", vec![e_a]).await.unwrap();
        store.upsert_entities("col-b", vec![e_b]).await.unwrap();

        store.delete_by_source_uri("col-a", "file://shared.md").await.unwrap();

        let gq = GraphQuery { entity_name: None, entity_type: None, max_hops: 1, relation_filter: None };
        assert!(store.query("col-a", &gq).await.unwrap().is_empty());
        assert_eq!(store.query("col-b", &gq).await.unwrap().len(), 1, "col-b unaffected");
    }

    #[tokio::test]
    async fn delete_collection_removes_entities_and_relations() {
        let (store, _tmp) = make_store();
        let id1 = EntityId::new();
        let id2 = EntityId::new();
        let e1 = Entity {
            id: id1.clone(), name: "Foo".into(), entity_type: "T".into(),
            canonical_id: None, source_chunks: vec![], source_uri: "file://x.md".into(), collection_id: "col".into(),
        };
        let e2 = Entity {
            id: id2.clone(), name: "Bar".into(), entity_type: "T".into(),
            canonical_id: None, source_chunks: vec![], source_uri: "file://y.md".into(), collection_id: "col".into(),
        };
        store.upsert_entities("col", vec![e1, e2]).await.unwrap();
        let rel = Relation {
            source: id1.clone(), relation_type: "links_to".into(), target: id2.clone(),
            confidence: 1.0, source_chunks: vec![],
        };
        store.upsert_relations("col", vec![rel]).await.unwrap();

        store.delete_collection("col").await.unwrap();

        let gq = GraphQuery { entity_name: None, entity_type: None, max_hops: 1, relation_filter: None };
        let results = store.query("col", &gq).await.unwrap();
        assert!(results.is_empty());
        let cols = store.list_collections().await.unwrap();
        assert!(!cols.contains(&"col".to_string()));
        let relations = store.get_relations(&id1).await.unwrap();
        assert!(relations.is_empty(), "relations must be cascade-removed when the collection is deleted");
    }
}
