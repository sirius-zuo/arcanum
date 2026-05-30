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

#[tokio::test]
async fn test_ever_seen_false_initially() {
    let tracker = DocumentHashTracker::new();
    assert!(!tracker.ever_seen("file:///doc.pdf").await);
}

#[tokio::test]
async fn test_record_then_ever_seen() {
    let tracker = DocumentHashTracker::new();
    tracker.record("file:///doc.pdf", b"content").await;
    assert!(tracker.ever_seen("file:///doc.pdf").await);
}

#[tokio::test]
async fn test_seen_unchanged_true_when_same_content() {
    let tracker = DocumentHashTracker::new();
    tracker.record("file:///doc.pdf", b"content").await;
    assert!(tracker.seen_unchanged("file:///doc.pdf", b"content").await);
}

#[tokio::test]
async fn test_seen_unchanged_false_when_content_changed() {
    let tracker = DocumentHashTracker::new();
    tracker.record("file:///doc.pdf", b"old").await;
    assert!(!tracker.seen_unchanged("file:///doc.pdf", b"new").await);
}
