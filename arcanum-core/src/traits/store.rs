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
}

#[async_trait]
pub trait TreeStore: Send + Sync {
    async fn insert_node(&self, collection: &str, node: TreeNode) -> Result<()>;
    async fn get_level(&self, collection: &str, level: u32) -> Result<Vec<TreeNode>>;
    async fn get_children(&self, node_id: &TreeNodeId) -> Result<Vec<TreeNode>>;
    /// Delete all tree nodes for the given source_uri in the collection. No-op if none exist.
    async fn delete_by_source_uri(&self, collection: &str, source_uri: &str) -> Result<()>;

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
