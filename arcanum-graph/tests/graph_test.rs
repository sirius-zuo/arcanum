use arcanum_graph::InMemoryGraphStore;
use arcanum_core::traits::*;
use arcanum_core::types::*;

#[tokio::test]
async fn test_upsert_and_query_entities() {
    let store = InMemoryGraphStore::new();
    let e1 = Entity {
        id: EntityId::new(), name: "Rust".to_string(),
        entity_type: "Language".to_string(), canonical_id: None, source_chunks: vec![],
        source_uri: "".to_string(), collection_id: "test-col".to_string(),
    };
    store.upsert_entities("test-col", vec![e1]).await.unwrap();

    let results = store.query("test-col", &GraphQuery {
        entity_name: Some("Rust".to_string()),
        entity_type: None,
        max_hops: 1,
        relation_filter: None,
    }).await.unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].name, "Rust");
}

#[tokio::test]
async fn test_upsert_relations() {
    let store = InMemoryGraphStore::new();
    let id1 = EntityId::new(); let id2 = EntityId::new();
    let e1 = Entity { id: id1.clone(), name: "Arcanum".into(), entity_type: "Project".into(), canonical_id: None, source_chunks: vec![], source_uri: "".to_string(), collection_id: "test-col".to_string() };
    let e2 = Entity { id: id2.clone(), name: "Rust".into(), entity_type: "Language".into(), canonical_id: None, source_chunks: vec![], source_uri: "".to_string(), collection_id: "test-col".to_string() };
    store.upsert_entities("test-col", vec![e1, e2]).await.unwrap();

    let rel = Relation { source: id1.clone(), relation_type: "written_in".into(), target: id2, confidence: 0.95, source_chunks: vec![ChunkId::new()] };
    store.upsert_relations("test-col", vec![rel]).await.unwrap();

    let relations = store.get_relations(&id1).await.unwrap();
    assert_eq!(relations.len(), 1);
    assert_eq!(relations[0].relation_type, "written_in");
}
