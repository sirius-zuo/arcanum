use arcanum_ingestion::FileLoader;
use arcanum_core::traits::{DocumentLoader, Source};
use std::io::Write;

#[tokio::test]
async fn test_file_loader_reads_markdown() {
    let mut tmp = tempfile::Builder::new().suffix(".md").tempfile().unwrap();
    tmp.write_all(b"# Hello\nWorld").unwrap();
    let loader = FileLoader::new();
    let source = Source::File(tmp.path().to_path_buf());
    assert!(loader.supports(&source));
    let doc = loader.load(&source).await.unwrap();
    assert_eq!(doc.mime_type, "text/markdown");
    assert!(!doc.content.is_empty());
}
