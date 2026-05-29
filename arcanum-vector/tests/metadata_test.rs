use arcanum_vector::SqliteMetadataStore;
use arcanum_core::types::*;

#[tokio::test]
async fn test_record_and_lookup_document() {
    let store = SqliteMetadataStore::new_in_memory().await.unwrap();
    let doc_id = DocumentId::new();
    store.record_document(&doc_id, "file://test.txt", "abc123hash", "default").await.unwrap();
    let hash = store.get_document_hash(&doc_id).await.unwrap();
    assert_eq!(hash, Some("abc123hash".to_string()));
}

#[tokio::test]
async fn test_unchanged_document_detected() {
    let store = SqliteMetadataStore::new_in_memory().await.unwrap();
    let doc_id = DocumentId::new();
    store.record_document(&doc_id, "file://test.txt", "hash1", "default").await.unwrap();
    let changed = store.is_document_changed(&doc_id, "hash1").await.unwrap();
    assert!(!changed);
    let changed = store.is_document_changed(&doc_id, "hash2").await.unwrap();
    assert!(changed);
}
