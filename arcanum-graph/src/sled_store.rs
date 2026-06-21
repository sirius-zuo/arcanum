use arcanum_core::{traits::GraphStore, traits::store::GraphQuery, types::*, ArcanumError, Result, traits::store::{relation_identity_key, relation_touches_removed_entity, merge_relation}};
use async_trait::async_trait;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use tokio::sync::RwLock;
use tracing::instrument;

fn entity_key(collection: &str, entity_id: &str) -> Result<Vec<u8>> {
    if collection.contains('\0') {
        return Err(ArcanumError::Config(format!(
            "collection name must not contain a NUL byte: {collection:?}"
        )));
    }
    Ok(format!("{collection}\0{entity_id}").into_bytes())
}

fn entity_prefix(collection: &str) -> Result<Vec<u8>> {
    if collection.contains('\0') {
        return Err(ArcanumError::Config(format!(
            "collection name must not contain a NUL byte: {collection:?}"
        )));
    }
    Ok(format!("{collection}\0").into_bytes())
}

macro_rules! storage_err {
    ($ctx:expr, $e:expr) => {
        ArcanumError::Storage(format!("{}: {}", $ctx, $e))
    };
}

/// Sled-backed GraphStore for persistent local dev/test use — no server process required.
/// Relations are stored globally (not collection-partitioned), matching Neo4jStore's real
/// MERGE semantics; entities stay collection-partitioned, matching Neo4jStore's e.collection filter.
pub struct SledGraphStore {
    db:          sled::Db,
    entities:    sled::Tree,
    relations:   sled::Tree,
    collections: sled::Tree,
    lock:        RwLock<()>,
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
        Ok(Self { db, entities, relations, collections, lock: RwLock::new(()) })
    }

    fn cascade_delete_relations(&self, removed_ids: &HashSet<String>) -> Result<()> {
        if removed_ids.is_empty() {
            return Ok(());
        }
        let items: Vec<(sled::IVec, sled::IVec)> = self.relations.iter()
            .collect::<sled::Result<Vec<_>>>()
            .map_err(|e| storage_err!("scan relations", e))?;
        for (key, value) in items {
            let relation: Relation = serde_json::from_slice(&value)
                .map_err(|e| storage_err!("deserialize relation", e))?;
            if relation_touches_removed_entity(removed_ids, &relation) {
                self.relations.remove(&key)
                    .map_err(|e| storage_err!("remove relation", e))?;
            }
        }
        Ok(())
    }
}

#[async_trait]
impl GraphStore for SledGraphStore {
    #[instrument(skip(self, entities), fields(store = "sled_graph", collection, entity_count = entities.len()), err)]
    async fn upsert_entities(&self, collection: &str, entities: Vec<Entity>) -> Result<()> {
        {
            let _guard = self.lock.write().await;
            for e in &entities {
                let key = entity_key(collection, &e.id.0.to_string())?;
                let value = serde_json::to_vec(e)
                    .map_err(|err| storage_err!("serialize entity", err))?;
                self.entities.insert(key, value)
                    .map_err(|err| storage_err!("insert entity", err))?;
            }
        }
        self.db.flush_async().await
            .map_err(|err| storage_err!("flush", err))?;
        Ok(())
    }

    #[instrument(skip(self, relations), fields(store = "sled_graph", collection, relation_count = relations.len()), err)]
    async fn upsert_relations(&self, _collection: &str, relations: Vec<Relation>) -> Result<()> {
        let mut to_store = Vec::with_capacity(relations.len());
        for r in relations {
            let source_exists = self.get_entity_by_id(&r.source).await?.is_some();
            let target_exists = self.get_entity_by_id(&r.target).await?.is_some();
            if source_exists && target_exists {
                to_store.push(r);
            } else {
                tracing::warn!(
                    store = "sled_graph",
                    source_exists, target_exists,
                    "upsert_relations skipping relation with missing endpoint entity",
                );
            }
        }
        {
            let _guard = self.lock.write().await;
            for r in to_store {
                let key = relation_identity_key(&r.source, &r.relation_type, &r.target);
                let merged = match self.relations.get(&key)
                    .map_err(|e| storage_err!("get relation for merge", e))?
                {
                    Some(bytes) => {
                        let existing: Relation = serde_json::from_slice(&bytes)
                            .map_err(|e| storage_err!("deserialize relation", e))?;
                        merge_relation(existing, r)
                    }
                    None => r,
                };
                let value = serde_json::to_vec(&merged)
                    .map_err(|e| storage_err!("serialize relation", e))?;
                self.relations.insert(key, value)
                    .map_err(|e| storage_err!("insert relation", e))?;
            }
        }
        self.db.flush_async().await
            .map_err(|e| storage_err!("flush", e))?;
        Ok(())
    }

    #[instrument(skip(self, q), fields(store = "sled_graph", collection, result_count), err)]
    async fn query(&self, collection: &str, q: &GraphQuery) -> Result<Vec<Entity>> {
        let _guard = self.lock.read().await;
        let prefix = entity_prefix(collection)?;
        let mut results = vec![];
        for item in self.entities.scan_prefix(&prefix) {
            let (_, value) = item.map_err(|e| storage_err!("scan entities", e))?;
            let entity: Entity = serde_json::from_slice(&value)
                .map_err(|e| storage_err!("deserialize entity", e))?;
            let name_ok = q.entity_name.as_deref().map(|n| entity.name.contains(n)).unwrap_or(true);
            let type_ok = q.entity_type.as_deref().map(|t| entity.entity_type == t).unwrap_or(true);
            if name_ok && type_ok {
                results.push(entity);
            }
        }
        if q.entity_name.is_none() && q.entity_type.is_none() {
            results.truncate(100);
        }
        tracing::Span::current().record("result_count", results.len());
        Ok(results)
    }

    async fn get_relations(&self, entity_id: &EntityId) -> Result<Vec<Relation>> {
        let _guard = self.lock.read().await;
        let prefix = format!("{}\0", entity_id.0).into_bytes();
        let mut results = vec![];
        for item in self.relations.scan_prefix(&prefix) {
            let (_, value) = item.map_err(|e| storage_err!("scan relations", e))?;
            let relation: Relation = serde_json::from_slice(&value)
                .map_err(|e| storage_err!("deserialize relation", e))?;
            results.push(relation);
        }
        Ok(results)
    }

    #[instrument(skip(self), fields(store = "sled_graph", collection, source_uri), err)]
    async fn delete_by_source_uri(&self, collection: &str, source_uri: &str) -> Result<()> {
        if source_uri.is_empty() {
            tracing::warn!(store = "sled_graph", "delete_by_source_uri called with empty source_uri — skipping");
            return Ok(());
        }
        {
            let _guard = self.lock.write().await;
            let items: Vec<(sled::IVec, sled::IVec)> = self.entities.scan_prefix(entity_prefix(collection)?)
                .collect::<sled::Result<Vec<_>>>()
                .map_err(|e| storage_err!("scan entities", e))?;
            let mut removed_ids: HashSet<String> = HashSet::new();
            for (key, value) in items {
                let entity: Entity = serde_json::from_slice(&value)
                    .map_err(|e| storage_err!("deserialize entity", e))?;
                if entity.source_uri == source_uri {
                    removed_ids.insert(entity.id.0.to_string());
                    self.entities.remove(&key)
                        .map_err(|e| storage_err!("remove entity", e))?;
                }
            }
            self.cascade_delete_relations(&removed_ids)?;

            // Remove ghost collection marker if no entities remain in this collection
            let remaining = self.entities.scan_prefix(entity_prefix(collection)?).next();
            if remaining.is_none() {
                self.collections.remove(collection.as_bytes())
                    .map_err(|e| storage_err!("remove ghost collection", e))?;
            }
        }
        self.db.flush_async().await
            .map_err(|e| storage_err!("flush", e))?;
        Ok(())
    }

    async fn list_collections(&self) -> Result<Vec<String>> {
        let _guard = self.lock.read().await;
        let mut names: HashSet<String> = HashSet::new();
        for item in self.collections.iter() {
            let (key, _) = item.map_err(|e| storage_err!("scan collections", e))?;
            names.insert(String::from_utf8_lossy(&key).to_string());
        }
        for item in self.entities.iter() {
            let (key, _) = item.map_err(|e| storage_err!("scan entities", e))?;
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
        {
            let _guard = self.lock.write().await;
            let already_marked = self.collections.contains_key(collection.as_bytes())
                .map_err(|e| storage_err!("check collection", e))?;
            let already_has_entities = self.entities.scan_prefix(entity_prefix(collection)?).next().is_some();
            if already_marked || already_has_entities {
                return Err(ArcanumError::AlreadyExists(
                    format!("collection '{}' already exists", collection),
                ));
            }
            self.collections.insert(collection.as_bytes(), Vec::<u8>::new())
                .map_err(|e| storage_err!("create collection", e))?;
        }
        self.db.flush_async().await
            .map_err(|e| storage_err!("flush", e))?;
        Ok(())
    }

    async fn count_documents(&self, collection: Option<&str>) -> Result<u64> {
        let _guard = self.lock.read().await;
        let items: Vec<(sled::IVec, sled::IVec)> = match collection {
            Some(col) => self.entities.scan_prefix(entity_prefix(col)?)
                .collect::<sled::Result<Vec<_>>>()
                .map_err(|e| storage_err!("scan entities", e))?,
            None => self.entities.iter()
                .collect::<sled::Result<Vec<_>>>()
                .map_err(|e| storage_err!("scan entities", e))?,
        };
        let mut uris: HashSet<String> = HashSet::new();
        for (_, value) in items {
            let entity: Entity = serde_json::from_slice(&value)
                .map_err(|e| storage_err!("deserialize entity", e))?;
            if !entity.source_uri.is_empty() {
                uris.insert(entity.source_uri);
            }
        }
        Ok(uris.len() as u64)
    }

    async fn count_documents_all(&self) -> Result<HashMap<String, u64>> {
        let _guard = self.lock.read().await;
        let mut map: HashMap<String, u64> = HashMap::new();
        for item in self.collections.iter() {
            let (key, _) = item.map_err(|e| storage_err!("scan collections", e))?;
            map.insert(String::from_utf8_lossy(&key).to_string(), 0);
        }
        let mut per_collection_uris: HashMap<String, HashSet<String>> = HashMap::new();
        for item in self.entities.iter() {
            let (key, value) = item.map_err(|e| storage_err!("scan entities", e))?;
            let key_str = String::from_utf8_lossy(&key);
            let collection = match key_str.find('\0') {
                Some(idx) => key_str[..idx].to_string(),
                None => continue,
            };
            let entity: Entity = serde_json::from_slice(&value)
                .map_err(|e| storage_err!("deserialize entity", e))?;
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
        {
            let _guard = self.lock.write().await;
            let items: Vec<(sled::IVec, sled::IVec)> = self.entities.scan_prefix(entity_prefix(collection)?)
                .collect::<sled::Result<Vec<_>>>()
                .map_err(|e| storage_err!("scan entities", e))?;
            let mut removed_ids: HashSet<String> = HashSet::new();
            for (key, value) in items {
                let entity: Entity = serde_json::from_slice(&value)
                    .map_err(|e| storage_err!("deserialize entity", e))?;
                removed_ids.insert(entity.id.0.to_string());
                self.entities.remove(&key)
                    .map_err(|e| storage_err!("remove entity", e))?;
            }
            self.collections.remove(collection.as_bytes())
                .map_err(|e| storage_err!("remove collection marker", e))?;
            self.cascade_delete_relations(&removed_ids)?;
        }
        self.db.flush_async().await
            .map_err(|e| storage_err!("flush", e))?;
        Ok(())
    }

    async fn get_entity_by_id(&self, entity_id: &EntityId) -> Result<Option<Entity>> {
        let _guard = self.lock.read().await;
        for item in self.entities.iter() {
            let (_, value) = item.map_err(|e| storage_err!("scan entities", e))?;
            let entity: Entity = serde_json::from_slice(&value)
                .map_err(|e| storage_err!("deserialize entity", e))?;
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
        let _guard = self.lock.read().await;
        let key = relation_identity_key(source_id, relation_type, target_id);
        match self.relations.get(&key)
            .map_err(|e| storage_err!("get relation", e))?
        {
            Some(bytes) => {
                let relation = serde_json::from_slice(&bytes)
                    .map_err(|e| storage_err!("deserialize relation", e))?;
                Ok(Some(relation))
            }
            None => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use tempfile::TempDir;

    fn make_store() -> (SledGraphStore, TempDir) {
        let tmp = TempDir::new().unwrap();
        let store = SledGraphStore::new(tmp.path()).unwrap();
        (store, tmp)
    }

    #[serial]
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

    #[serial]
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

    #[serial]
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

    #[serial]
    #[tokio::test]
    async fn upsert_relations_merges_evidence_instead_of_overwriting() {
        let (store, _tmp) = make_store();
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

    #[serial]
    #[tokio::test]
    async fn upsert_relations_skips_relation_with_missing_source_entity() {
        let (store, _tmp) = make_store();
        let missing_src = EntityId::new();
        let tgt = EntityId::new();
        let e_tgt = Entity {
            id: tgt.clone(), name: "Target".into(), entity_type: "T".into(),
            canonical_id: None, source_chunks: vec![], source_uri: "".into(), collection_id: "col".into(),
        };
        store.upsert_entities("col", vec![e_tgt]).await.unwrap();
        let rel = Relation {
            source: missing_src.clone(), relation_type: "X".into(), target: tgt.clone(),
            confidence: 1.0, source_chunks: vec![],
        };
        store.upsert_relations("col", vec![rel]).await.unwrap();

        let relations = store.get_relations(&missing_src).await.unwrap();
        assert!(relations.is_empty(), "a relation whose source entity doesn't exist must be silently skipped");
    }

    #[serial]
    #[tokio::test]
    async fn upsert_relations_skips_relation_with_missing_target_entity() {
        let (store, _tmp) = make_store();
        let src = EntityId::new();
        let missing_tgt = EntityId::new();
        let e_src = Entity {
            id: src.clone(), name: "Source".into(), entity_type: "T".into(),
            canonical_id: None, source_chunks: vec![], source_uri: "".into(), collection_id: "col".into(),
        };
        store.upsert_entities("col", vec![e_src]).await.unwrap();
        let rel = Relation {
            source: src.clone(), relation_type: "X".into(), target: missing_tgt.clone(),
            confidence: 1.0, source_chunks: vec![],
        };
        store.upsert_relations("col", vec![rel]).await.unwrap();

        let relations = store.get_relations(&src).await.unwrap();
        assert!(relations.is_empty(), "a relation whose target entity doesn't exist must be silently skipped");
    }

    #[serial]
    #[tokio::test]
    async fn upsert_relations_keeps_relation_when_both_entities_exist() {
        let (store, _tmp) = make_store();
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
            source: src.clone(), relation_type: "X".into(), target: tgt.clone(),
            confidence: 1.0, source_chunks: vec![],
        };
        store.upsert_relations("col", vec![rel]).await.unwrap();

        let relations = store.get_relations(&src).await.unwrap();
        assert_eq!(relations.len(), 1, "a relation must still be stored when both endpoints exist");
    }

    #[serial]
    #[tokio::test]
    async fn get_relation_looks_up_by_source_type_target() {
        let (store, _tmp) = make_store();
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

    #[serial]
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

    #[serial]
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

    #[serial]
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

    #[serial]
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

    #[serial]
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

    #[serial]
    #[tokio::test]
    async fn delete_by_source_uri_cascades_relation_across_collections() {
        let (store, _tmp) = make_store();
        let e_a_id = EntityId::new();
        let e_b_id = EntityId::new();
        let e_a = Entity {
            id: e_a_id.clone(), name: "DocA".into(), entity_type: "Doc".into(),
            canonical_id: None, source_chunks: vec![], source_uri: "file://a.md".into(), collection_id: "col-a".into(),
        };
        let e_b = Entity {
            id: e_b_id.clone(), name: "DocB".into(), entity_type: "Doc".into(),
            canonical_id: None, source_chunks: vec![], source_uri: "file://b.md".into(), collection_id: "col-b".into(),
        };
        store.upsert_entities("col-a", vec![e_a]).await.unwrap();
        store.upsert_entities("col-b", vec![e_b]).await.unwrap();

        let rel = Relation {
            source: e_a_id.clone(), relation_type: "links_to".into(), target: e_b_id.clone(),
            confidence: 1.0, source_chunks: vec![],
        };
        store.upsert_relations("col-a", vec![rel]).await.unwrap();

        // Delete e_a from col-a; e_b remains in col-b.
        store.delete_by_source_uri("col-a", "file://a.md").await.unwrap();

        // Relation should be cascade-deleted despite reaching across collections.
        let relations = store.get_relations(&e_a_id).await.unwrap();
        assert!(relations.is_empty(), "relation must be cascade-deleted across collections");

        // Confirm e_b is still present in col-b (genuine cross-collection scenario).
        let gq = GraphQuery { entity_name: None, entity_type: None, max_hops: 1, relation_filter: None };
        let col_b_results = store.query("col-b", &gq).await.unwrap();
        assert_eq!(col_b_results.len(), 1, "e_b must remain in col-b after deleting e_a from col-a");
        assert_eq!(col_b_results[0].name, "DocB");
    }

    #[serial]
    #[tokio::test]
    async fn data_survives_drop_and_reopen() {
        let tmp = TempDir::new().unwrap();
        let entity_id = EntityId::new();

        {
            let store = SledGraphStore::new(tmp.path()).unwrap();
            let entity = Entity {
                id: entity_id.clone(), name: "Persisted Entity".into(), entity_type: "T".into(),
                canonical_id: None, source_chunks: vec![], source_uri: "file://doc.md".into(),
                collection_id: "col".into(),
            };
            store.upsert_entities("col", vec![entity]).await.unwrap();
            let rel = Relation {
                source: entity_id.clone(), relation_type: "SELF".into(), target: entity_id.clone(),
                confidence: 1.0, source_chunks: vec![],
            };
            store.upsert_relations("col", vec![rel]).await.unwrap();
            // store is dropped here, closing the sled db.
        }

        let reopened = SledGraphStore::new(tmp.path()).unwrap();
        let found = reopened.get_entity_by_id(&entity_id).await.unwrap();
        assert!(found.is_some(), "entity must survive a drop + reopen of the store");
        assert_eq!(found.unwrap().name, "Persisted Entity");

        let relations = reopened.get_relations(&entity_id).await.unwrap();
        assert_eq!(relations.len(), 1, "relation must survive a drop + reopen of the store");
    }

    #[test]
    fn sled_graph_store_is_send_and_sync() {
        fn _assert_send_sync<T: Send + Sync>() {}
        _assert_send_sync::<SledGraphStore>();
    }

    #[serial]
    #[tokio::test]
    async fn sled_nul_byte_in_collection_name_is_rejected() {
        let (store, _tmp) = make_store();
        let entity = Entity {
            id: EntityId::new(), name: "X".into(), entity_type: "T".into(),
            canonical_id: None, source_chunks: vec![], source_uri: "file://x.md".into(),
            collection_id: "col".into(),
        };
        let result = store.upsert_entities("bad\0collection", vec![entity]).await;
        assert!(result.is_err(), "collection names containing NUL bytes must be rejected");
    }

    #[serial]
    #[tokio::test]
    async fn delete_by_source_uri_removes_ghost_collection_when_never_explicitly_created() {
        let (store, _tmp) = make_store();
        // Upsert entity — this only inserts the entity key, no explicit collection marker.
        let entity = Entity {
            id: EntityId::new(), name: "Ephemeral".into(), entity_type: "T".into(),
            canonical_id: None, source_chunks: vec![], source_uri: "file://ephemeral.md".into(),
            collection_id: "ghost-col".into(),
        };
        store.upsert_entities("ghost-col", vec![entity]).await.unwrap();

        // Verify collection appears in list_collections.
        let cols = store.list_collections().await.unwrap();
        assert!(cols.contains(&"ghost-col".to_string()));

        // Delete the only entity in the collection.
        store.delete_by_source_uri("ghost-col", "file://ephemeral.md").await.unwrap();

        // Ghost collection marker should be cleaned up.
        let cols = store.list_collections().await.unwrap();
        assert!(!cols.contains(&"ghost-col".to_string()), "ghost collection should be removed");
    }
}
