use arcanum_ingestion::DocumentHashTracker;

#[test]
fn test_hash_is_deterministic() {
    let h1 = DocumentHashTracker::compute_hash(b"hello world");
    let h2 = DocumentHashTracker::compute_hash(b"hello world");
    assert_eq!(h1, h2);
}

#[test]
fn test_different_content_different_hash() {
    let h1 = DocumentHashTracker::compute_hash(b"abc");
    let h2 = DocumentHashTracker::compute_hash(b"def");
    assert_ne!(h1, h2);
}
