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
        use sha2::{Sha256, Digest};
        let mut hasher = Sha256::new();
        hasher.update(&self.content);
        hex::encode(hasher.finalize())
    }

    /// Construct a minimal RawDocument for use in tests.
    /// Uses a fresh DocumentId and empty metadata; callers only specify what the test cares about.
    pub fn for_test(content: impl Into<Vec<u8>>, mime_type: impl Into<String>) -> Self {
        Self {
            id:         DocumentId::new(),
            content:    content.into(),
            mime_type:  mime_type.into(),
            source_uri: "test://fixture".into(),
            metadata:   HashMap::new(),
        }
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
    pub provenance: super::provenance::ChunkProvenance,
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
    use crate::types::provenance::ChunkProvenance;

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
            provenance: ChunkProvenance::default(),
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

    #[test]
    fn test_content_hash_is_sha256() {
        let doc = RawDocument {
            id: DocumentId::new(),
            content: b"hello world".to_vec(),
            mime_type: "text/plain".to_string(),
            source_uri: "test://x".to_string(),
            metadata: Default::default(),
        };
        // Verify it's 64 hex chars (SHA-256 output length)
        assert_eq!(doc.content_hash().len(), 64, "SHA-256 hex is 64 chars");
        // Verify same content produces same hash (deterministic across processes):
        assert_eq!(doc.content_hash(), doc.content_hash());
        // Verify different content produces different hash:
        let doc2 = RawDocument {
            id: DocumentId::new(),
            content: b"hello WORLD".to_vec(),
            mime_type: "text/plain".to_string(),
            source_uri: "test://y".to_string(),
            metadata: Default::default(),
        };
        assert_ne!(doc.content_hash(), doc2.content_hash());
    }
}
