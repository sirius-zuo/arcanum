use arcanum_core::types::*;
use arcanum_tree::{InMemoryTreeStore, RaptorBuilder};
use arcanum_core::traits::TreeStore;

#[tokio::test]
async fn test_tree_store_insert_and_level_query() {
    let store = InMemoryTreeStore::new();
    let node = TreeNode {
        id: TreeNodeId::new(), level: 0,
        text: "leaf chunk".to_string(),
        vector: Vector(vec![0.1, 0.2]),
        parent: None, children: vec![],
        cluster_centroid: None,
        source_uri: "".to_string(),
        leaf_chunk_ids: vec![],
    };
    store.insert_node("test", node.clone()).await.unwrap();
    let level0 = store.get_level("test", 0).await.unwrap();
    assert_eq!(level0.len(), 1);
    assert_eq!(level0[0].text, "leaf chunk");
}

#[tokio::test]
async fn test_raptor_builds_tree_levels() {
    let store = std::sync::Arc::new(InMemoryTreeStore::new());
    let builder = RaptorBuilder::new(store.clone(), 3);

    let chunks: Vec<(String, Vector)> = (0..4).map(|i| {
        (format!("chunk {i}"), Vector(vec![i as f32 * 0.1, i as f32 * 0.2]))
    }).collect();

    builder.build("test", "test://doc", chunks).await.unwrap();

    let level0 = store.get_level("test", 0).await.unwrap();
    assert_eq!(level0.len(), 4);
    let level1 = store.get_level("test", 1).await.unwrap();
    assert!(!level1.is_empty());
}
