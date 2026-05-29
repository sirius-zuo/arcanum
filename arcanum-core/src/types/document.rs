use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ChunkId(pub Uuid);
impl ChunkId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DocumentId(pub Uuid);
impl DocumentId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CollectionId(pub String);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawDocument {
    pub id: DocumentId,
    pub content: Vec<u8>,
    pub mime_type: String,
    pub source_uri: String,
    pub metadata: HashMap<String, String>,
}

impl RawDocument {
    pub fn content_hash(&self) -> String {
        use std::hash::{Hash, Hasher};
        use std::collections::hash_map::DefaultHasher;
        let mut h = DefaultHasher::new();
        self.content.hash(&mut h);
        format!("{:x}", h.finish())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkPosition {
    pub start: usize,
    pub end: usize,
    pub index: usize,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ChunkMetadata(pub HashMap<String, serde_json::Value>);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chunk {
    pub id: ChunkId,
    pub text: String,
    pub document_id: DocumentId,
    pub collection_id: CollectionId,
    pub position: ChunkPosition,
    pub metadata: ChunkMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexedChunk {
    pub chunk: Chunk,
    pub vector: Vector,
    pub token_vectors: Option<Vec<Vector>>,
    pub store_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetrievedChunk {
    pub indexed_chunk: IndexedChunk,
    pub score: f32,
    pub strategy: RetrievalStrategy,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum RetrievalStrategy {
    Vector,
    Bm25,
    ColBert,
    Raptor,
    Graph,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Vector(pub Vec<f32>);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chunk_id_is_unique() {
        let a = ChunkId::new();
        let b = ChunkId::new();
        assert_ne!(a, b);
    }

    #[test]
    fn test_chunk_construction() {
        let chunk = Chunk {
            id: ChunkId::new(),
            text: "Hello world".to_string(),
            document_id: DocumentId::new(),
            collection_id: CollectionId("default".to_string()),
            position: ChunkPosition {
                start: 0,
                end: 11,
                index: 0,
            },
            metadata: ChunkMetadata::default(),
        };
        assert_eq!(chunk.text, "Hello world");
        assert_eq!(chunk.position.index, 0);
    }

    #[test]
    fn test_raw_document_hash() {
        let doc = RawDocument {
            id: DocumentId::new(),
            content: b"test content".to_vec(),
            mime_type: "text/plain".to_string(),
            source_uri: "file://test.txt".to_string(),
            metadata: Default::default(),
        };
        assert_eq!(doc.content_hash(), doc.content_hash()); // deterministic
    }
}
