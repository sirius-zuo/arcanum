use arcanum_core::types::*;
use arcanum_core::traits::*;
use arcanum_vector::LanceDbStore;

#[tokio::test]
async fn test_upsert_and_search() {
    let dir = tempfile::tempdir().unwrap();
    let store = LanceDbStore::new(dir.path().to_str().unwrap()).await.unwrap();

    let chunk = IndexedChunk {
        chunk: Chunk {
            id: ChunkId::new(),
            text: "rust is fast".to_string(),
            document_id: DocumentId::new(),
            collection_id: CollectionId("test".into()),
            position: ChunkPosition { start: 0, end: 12, index: 0 },
            metadata: ChunkMetadata::default(),
        },
        vector: Vector(vec![0.1, 0.2, 0.3]),
        token_vectors: None,
        store_id: String::new(),
    };

    store.upsert("test", vec![chunk]).await.unwrap();

    let results = store.search("test", &VectorQuery {
        vector: Vector(vec![0.1, 0.2, 0.3]),
        top_k: 5,
        filters: vec![],
    }).await.unwrap();

    assert!(!results.is_empty());
    assert_eq!(results[0].chunk.chunk.text, "rust is fast");
}
