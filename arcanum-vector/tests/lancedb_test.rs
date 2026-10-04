use arcanum_core::traits::*;
use arcanum_core::types::*;
use arcanum_vector::LanceDbStore;

#[tokio::test]
async fn test_upsert_and_search() {
    let dir = tempfile::tempdir().unwrap();
    let store = LanceDbStore::new(dir.path().to_str().unwrap())
        .await
        .unwrap();

    let chunk = IndexedChunk {
        chunk: Chunk {
            id: ChunkId::new(),
            text: "rust is fast".to_string(),
            document_id: DocumentId::new(),
            collection_id: CollectionId("test".into()),
            position: ChunkPosition {
                start: 0,
                end: 12,
                index: 0,
            },
            metadata: ChunkMetadata::default(),
            provenance: Default::default(),
        },
        vector: Vector(vec![0.1, 0.2, 0.3]),
        token_vectors: None,
        store_id: String::new(),
    };

    store.upsert("test", vec![chunk]).await.unwrap();

    let results = store
        .search(
            "test",
            &VectorQuery {
                vector: Vector(vec![0.1, 0.2, 0.3]),
                top_k: 5,
                filters: vec![],
            },
        )
        .await
        .unwrap();

    assert!(!results.is_empty());
    assert_eq!(results[0].chunk.chunk.text, "rust is fast");
}

fn indexed(text: &str, v: f32) -> IndexedChunk {
    IndexedChunk {
        chunk: Chunk {
            id: ChunkId::new(),
            text: text.to_string(),
            document_id: DocumentId::new(),
            collection_id: CollectionId("race".into()),
            position: ChunkPosition {
                start: 0,
                end: text.len(),
                index: 0,
            },
            metadata: ChunkMetadata::default(),
            provenance: Default::default(),
        },
        vector: Vector(vec![v, 0.2, 0.3]),
        token_vectors: None,
        store_id: String::new(),
    }
}

/// Concurrent first upserts to a collection whose table does not exist yet must
/// all succeed: only one task may create the table, the rest append to it.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_first_upserts_all_succeed() {
    let dir = tempfile::tempdir().unwrap();
    let store = std::sync::Arc::new(
        LanceDbStore::new(dir.path().to_str().unwrap())
            .await
            .unwrap(),
    );

    let mut tasks = Vec::new();
    for i in 0..8 {
        let store = store.clone();
        tasks.push(tokio::spawn(async move {
            store
                .upsert(
                    "race",
                    vec![indexed(&format!("chunk {i}"), i as f32 / 10.0)],
                )
                .await
        }));
    }
    for t in tasks {
        t.await.unwrap().expect("concurrent first upsert failed");
    }

    let results = store
        .search(
            "race",
            &VectorQuery {
                vector: Vector(vec![0.0, 0.2, 0.3]),
                top_k: 20,
                filters: vec![],
            },
        )
        .await
        .unwrap();
    assert_eq!(results.len(), 8, "every concurrent upsert must be stored");
}
