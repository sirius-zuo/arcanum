use arcanum_ingestion::{FixedSizeChunker, SemanticChunker};
use arcanum_core::traits::Chunker;
use arcanum_core::types::*;

fn make_doc(text: &str) -> RawDocument {
    RawDocument { id: DocumentId::new(), content: text.as_bytes().to_vec(),
        mime_type: "text/plain".into(), source_uri: "test".into(), metadata: Default::default() }
}

#[tokio::test]
async fn test_fixed_size_chunks_count() {
    let chunker = FixedSizeChunker::new(20, 5);
    let doc = make_doc("Hello world this is a test of chunking behavior");
    let chunks = chunker.chunk(&doc).await.unwrap();
    assert!(chunks.len() >= 2);
    for c in &chunks { assert!(c.text.len() <= 30); }
}

#[tokio::test]
async fn test_fixed_chunk_positions_are_sequential() {
    let chunker = FixedSizeChunker::new(10, 0);
    let doc = make_doc("abcdefghij klmnopqrst");
    let chunks = chunker.chunk(&doc).await.unwrap();
    for (i, c) in chunks.iter().enumerate() {
        assert_eq!(c.position.index, i);
    }
}

#[tokio::test]
async fn test_empty_document_returns_no_chunks() {
    let chunker = FixedSizeChunker::new(100, 0);
    let doc = make_doc("");
    let chunks = chunker.chunk(&doc).await.unwrap();
    assert!(chunks.is_empty());
}
