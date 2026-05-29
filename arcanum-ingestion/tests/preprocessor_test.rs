use arcanum_ingestion::HtmlCleaner;
use arcanum_core::traits::Preprocessor;
use arcanum_core::types::*;

#[tokio::test]
async fn test_html_cleaner_strips_tags() {
    let cleaner = HtmlCleaner::new();
    let doc = RawDocument {
        id: DocumentId::new(),
        content: b"<h1>Title</h1><p>Hello <b>world</b></p>".to_vec(),
        mime_type: "text/html".into(),
        source_uri: "test".into(),
        metadata: Default::default(),
    };
    let processed = cleaner.process(doc).await.unwrap();
    let text = String::from_utf8(processed.content).unwrap();
    assert!(text.contains("Title"));
    assert!(text.contains("Hello"));
    assert!(!text.contains("<b>"));
}
