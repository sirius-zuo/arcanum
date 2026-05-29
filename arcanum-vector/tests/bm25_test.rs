use arcanum_vector::Bm25Index;

#[tokio::test]
async fn test_bm25_index_and_search() {
    let dir = tempfile::tempdir().unwrap();
    let idx = Bm25Index::new(dir.path().to_str().unwrap()).unwrap();

    idx.index_chunks(vec![
        ("chunk-1".to_string(), "the quick brown fox".to_string()),
        ("chunk-2".to_string(), "jumps over the lazy dog".to_string()),
    ]).unwrap();

    let results = idx.search("quick fox", 5).unwrap();
    assert!(!results.is_empty());
    assert_eq!(results[0].0, "chunk-1");
}
