use async_trait::async_trait;
use crate::types::*;
use crate::Result;

#[derive(Debug, Clone)]
pub struct VectorQuery {
    pub vector: Vector,
    pub top_k: usize,
    pub filters: Vec<MetadataFilter>,
}

#[derive(Debug, Clone)]
pub struct ScoredChunk {
    pub chunk: IndexedChunk,
    pub score: f32,
}

#[async_trait]
pub trait VectorStore: Send + Sync {
    async fn upsert(&self, collection: &str, chunks: Vec<IndexedChunk>) -> Result<()>;
    async fn search(&self, collection: &str, query: &VectorQuery) -> Result<Vec<ScoredChunk>>;
    async fn delete(&self, collection: &str, ids: &[ChunkId]) -> Result<()>;
    async fn collection_exists(&self, collection: &str) -> Result<bool>;
    /// Delete all chunks for the given source_uri in the collection. No-op if none exist.
    async fn delete_by_source_uri(&self, collection: &str, source_uri: &str) -> Result<()>;

    /// Returns all collection names in this store, including empty collections.
    async fn list_collections(&self) -> Result<Vec<String>> { Ok(vec![]) }

    /// Create a new empty collection. Returns `AlreadyExists` if the name is taken.
    async fn create_collection(&self, _collection: &str) -> Result<()> { Ok(()) }

    /// Count distinct documents (by document_id) in this store.
    /// `None` → total across all collections; `Some("col")` → count for that collection.
    async fn count_documents(&self, _collection: Option<&str>) -> Result<u64> { Ok(0) }

    /// Delete all data for the given collection. Idempotent — no-op if it does not exist.
    async fn delete_collection(&self, _collection: &str) -> Result<()> { Ok(()) }
}

#[derive(Debug, Clone)]
pub struct GraphQuery {
    pub entity_name: Option<String>,
    pub entity_type: Option<String>,
    pub max_hops: u32,
    pub relation_filter: Option<String>,
}

#[async_trait]
pub trait GraphStore: Send + Sync {
    async fn upsert_entities(&self, collection: &str, entities: Vec<Entity>) -> Result<()>;
    async fn upsert_relations(&self, collection: &str, relations: Vec<Relation>) -> Result<()>;
    async fn query(&self, collection: &str, q: &GraphQuery) -> Result<Vec<Entity>>;
    async fn get_relations(&self, entity_id: &EntityId) -> Result<Vec<Relation>>;
    /// Delete all entities (and their relations) for the given source_uri in the collection.
    async fn delete_by_source_uri(&self, collection: &str, source_uri: &str) -> Result<()>;

    /// Returns all collection names, including empty ones.
    async fn list_collections(&self) -> Result<Vec<String>> { Ok(vec![]) }
    /// Create a new empty collection. Returns `AlreadyExists` if name is taken.
    async fn create_collection(&self, _collection: &str) -> Result<()> { Ok(()) }
    /// Count distinct source_uri values. None = whole store; Some(col) = one collection.
    async fn count_documents(&self, _collection: Option<&str>) -> Result<u64> { Ok(0) }
    /// Count distinct source_uri values per collection in a single operation.
    /// Returns a map of collection_name → document_count.
    /// The default impl calls list_collections + count_documents(Some) in a loop.
    async fn count_documents_all(&self) -> Result<std::collections::HashMap<String, u64>> {
        let cols = self.list_collections().await?;
        let mut map = std::collections::HashMap::new();
        for col in cols {
            let count = self.count_documents(Some(&col)).await?;
            map.insert(col, count);
        }
        Ok(map)
    }
    /// Delete all data for the collection. Idempotent.
    async fn delete_collection(&self, _collection: &str) -> Result<()> { Ok(()) }

    /// Look up a single entity by its UUID, including source_chunks.
    async fn get_entity_by_id(&self, _entity_id: &EntityId) -> Result<Option<Entity>> {
        Ok(None)
    }

    /// Look up a specific directed relation by endpoints and type.
    async fn get_relation(
        &self,
        source_id:     &EntityId,
        relation_type: &str,
        target_id:     &EntityId,
    ) -> Result<Option<Relation>> {
        let _ = (source_id, relation_type, target_id);
        Ok(None)
    }
}

/// Builds the canonical, flat (not collection-scoped) identity key for a
/// relation — matching `Neo4jStore`'s `MERGE`-by-`(source, relation_type,
/// target)` semantics. `EntityId`'s `Display` format is always exactly 36
/// ASCII bytes (lowercase hex + hyphens) and can never contain a `\0`, so
/// this key is unambiguous regardless of what bytes `relation_type`
/// contains: the fixed-width, NUL-free source/target segments pin the
/// string's prefix and suffix, which forces the middle segment — and
/// therefore the whole key — to be unique per distinct triple.
pub fn relation_identity_key(
    source: &EntityId,
    relation_type: &str,
    target: &EntityId,
) -> Vec<u8> {
    format!("{}\0{}\0{}", source.0, relation_type, target.0).into_bytes()
}

/// True if `relation` touches (as source or target) any entity id in
/// `removed_ids` — i.e. it must be cascade-deleted, matching `Neo4jStore`'s
/// `DETACH DELETE` semantics (which removes relationships regardless of
/// which side of the relationship the deleted entity was on, and regardless
/// of which collection the relationship's `collection` property names).
pub fn relation_touches_removed_entity(
    removed_ids: &std::collections::HashSet<String>,
    relation: &Relation,
) -> bool {
    removed_ids.contains(&relation.source.0.to_string())
        || removed_ids.contains(&relation.target.0.to_string())
}

/// Merges a newly-upserted relation into an already-stored relation for the
/// same `(source, relation_type, target)` identity, preserving evidence from
/// both instead of letting the newer upsert silently discard the older
/// one's `source_chunks`/`confidence`. Identity fields (source, relation_type,
/// target) are taken from `incoming` (they're equal to `existing`'s by
/// construction — both have already been matched on the same identity key).
pub fn merge_relation(existing: Relation, incoming: Relation) -> Relation {
    let mut source_chunks = existing.source_chunks;
    for c in incoming.source_chunks {
        if !source_chunks.contains(&c) {
            source_chunks.push(c);
        }
    }
    Relation {
        source: incoming.source,
        relation_type: incoming.relation_type,
        target: incoming.target,
        confidence: existing.confidence.max(incoming.confidence),
        source_chunks,
    }
}

#[async_trait]
pub trait TreeStore: Send + Sync {
    async fn insert_node(&self, collection: &str, node: TreeNode) -> Result<()>;
    async fn get_level(&self, collection: &str, level: u32) -> Result<Vec<TreeNode>>;
    async fn get_children(&self, node_id: &TreeNodeId) -> Result<Vec<TreeNode>>;
    /// Delete all tree nodes for the given source_uri in the collection. No-op if none exist.
    async fn delete_by_source_uri(&self, collection: &str, source_uri: &str) -> Result<()>;

    /// Look up a single tree node by its UUID. Returns None if not found.
    async fn get_by_id(&self, _node_id: &TreeNodeId) -> Result<Option<TreeNode>> {
        Ok(None)
    }

    async fn list_collections(&self) -> Result<Vec<String>> { Ok(vec![]) }
    async fn create_collection(&self, _collection: &str) -> Result<()> { Ok(()) }
    async fn count_documents(&self, _collection: Option<&str>) -> Result<u64> { Ok(0) }
    async fn delete_collection(&self, _collection: &str) -> Result<()> { Ok(()) }
}

#[async_trait]
pub trait SecretStore: Send + Sync {
    async fn get(&self, key: &str) -> Result<String>;
    async fn reload(&self) -> Result<()>;
}

#[cfg(test)]
mod helper_tests {
    use super::*;
    use crate::types::{ChunkId, EntityId, Relation};

    #[test]
    fn relation_identity_key_is_stable_and_distinct() {
        let a = EntityId::new();
        let b = EntityId::new();
        let k1 = relation_identity_key(&a, "WORKS_AT", &b);
        let k2 = relation_identity_key(&a, "WORKS_AT", &b);
        let k3 = relation_identity_key(&a, "MANAGES", &b);
        assert_eq!(k1, k2, "same triple must produce the same key");
        assert_ne!(k1, k3, "different relation_type must produce a different key");
    }

    #[test]
    fn relation_identity_key_survives_embedded_nul_in_relation_type() {
        let a = EntityId::new();
        let b = EntityId::new();
        let weird = relation_identity_key(&a, "FOO\0BAR", &b);
        let plain_foo = relation_identity_key(&a, "FOO", &b);
        let plain_bar = relation_identity_key(&a, "BAR", &b);
        assert_ne!(weird, plain_foo);
        assert_ne!(weird, plain_bar);
    }

    #[test]
    fn relation_touches_removed_entity_checks_both_endpoints() {
        let src = EntityId::new();
        let tgt = EntityId::new();
        let other = EntityId::new();
        let rel = Relation {
            source: src.clone(), relation_type: "X".into(), target: tgt.clone(),
            confidence: 1.0, source_chunks: vec![],
        };
        let mut removed = std::collections::HashSet::new();
        removed.insert(src.0.to_string());
        assert!(relation_touches_removed_entity(&removed, &rel), "source match must cascade");

        let mut removed_target = std::collections::HashSet::new();
        removed_target.insert(tgt.0.to_string());
        assert!(relation_touches_removed_entity(&removed_target, &rel), "target match must cascade");

        let mut removed_other = std::collections::HashSet::new();
        removed_other.insert(other.0.to_string());
        assert!(!relation_touches_removed_entity(&removed_other, &rel), "unrelated id must not cascade");
    }

    #[test]
    fn merge_relation_unions_source_chunks_and_keeps_max_confidence() {
        let src = EntityId::new();
        let tgt = EntityId::new();
        let c1 = ChunkId::new();
        let c2 = ChunkId::new();
        let existing = Relation {
            source: src.clone(), relation_type: "WORKS_AT".into(), target: tgt.clone(),
            confidence: 0.5, source_chunks: vec![c1.clone()],
        };
        let incoming = Relation {
            source: src.clone(), relation_type: "WORKS_AT".into(), target: tgt.clone(),
            confidence: 0.9, source_chunks: vec![c2.clone()],
        };
        let merged = merge_relation(existing, incoming);
        assert_eq!(merged.confidence, 0.9, "merge must keep the higher confidence");
        assert_eq!(merged.source_chunks.len(), 2, "merge must union source_chunks, not overwrite");
        assert!(merged.source_chunks.contains(&c1));
        assert!(merged.source_chunks.contains(&c2));
    }

    #[test]
    fn merge_relation_does_not_duplicate_shared_chunk() {
        let src = EntityId::new();
        let tgt = EntityId::new();
        let shared = ChunkId::new();
        let existing = Relation {
            source: src.clone(), relation_type: "WORKS_AT".into(), target: tgt.clone(),
            confidence: 0.5, source_chunks: vec![shared.clone()],
        };
        let incoming = Relation {
            source: src.clone(), relation_type: "WORKS_AT".into(), target: tgt.clone(),
            confidence: 0.9, source_chunks: vec![shared.clone()],
        };
        let merged = merge_relation(existing, incoming);
        assert_eq!(merged.source_chunks.len(), 1, "re-citing the same chunk must not duplicate it");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    struct InMemoryVectorStore(Mutex<HashMap<String, Vec<IndexedChunk>>>);

    #[async_trait::async_trait]
    impl VectorStore for InMemoryVectorStore {
        async fn upsert(&self, collection: &str, chunks: Vec<IndexedChunk>) -> crate::Result<()> {
            self.0.lock().unwrap().entry(collection.to_string()).or_default().extend(chunks);
            Ok(())
        }
        async fn search(&self, collection: &str, query: &VectorQuery) -> crate::Result<Vec<ScoredChunk>> {
            let store = self.0.lock().unwrap();
            let chunks = store.get(collection).cloned().unwrap_or_default();
            Ok(chunks.into_iter().take(query.top_k).map(|c| ScoredChunk { chunk: c, score: 0.9 }).collect())
        }
        async fn delete(&self, _collection: &str, _ids: &[ChunkId]) -> crate::Result<()> { Ok(()) }
        async fn collection_exists(&self, collection: &str) -> crate::Result<bool> {
            Ok(self.0.lock().unwrap().contains_key(collection))
        }
        async fn delete_by_source_uri(&self, _: &str, _: &str) -> crate::Result<()> { Ok(()) }
    }

    #[tokio::test]
    async fn test_vector_store_upsert_and_search() {
        let store = InMemoryVectorStore(Mutex::new(HashMap::new()));
        let chunk = IndexedChunk {
            chunk: Chunk {
                id: ChunkId::new(), text: "hello".into(),
                document_id: DocumentId::new(),
                collection_id: CollectionId("test".into()),
                position: ChunkPosition { start: 0, end: 5, index: 0 },
                metadata: ChunkMetadata::default(),
                provenance: Default::default(),
            },
            vector: Vector(vec![0.1, 0.2]),
            token_vectors: None,
            store_id: "".into(),
        };
        store.upsert("test", vec![chunk]).await.unwrap();
        let results = store.search("test", &VectorQuery { vector: Vector(vec![0.1, 0.2]), top_k: 5, filters: vec![] }).await.unwrap();
        assert_eq!(results.len(), 1);
    }
}
