use arcanum_core::{traits::Preprocessor, types::{DocumentId, RawDocument}, Result};
use async_trait::async_trait;

pub struct LanguageDetector;

impl LanguageDetector {
    pub fn new() -> Self { Self }
}

#[async_trait]
impl Preprocessor for LanguageDetector {
    async fn process(&self, mut doc: RawDocument) -> Result<RawDocument> {
        let text = String::from_utf8_lossy(&doc.content);
        if let Some(info) = whatlang::detect(&text) {
            doc.metadata.insert("lang".to_string(), info.lang().code().to_string());
            doc.metadata.insert("lang_confidence".to_string(), format!("{:.3}", info.confidence()));
        }
        Ok(doc)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_doc(text: &str) -> RawDocument {
        RawDocument {
            id: DocumentId::new(),
            content: text.as_bytes().to_vec(),
            mime_type: "text/plain".to_string(),
            source_uri: "test://x".to_string(),
            metadata: Default::default(),
        }
    }

    #[tokio::test]
    async fn test_language_detector_adds_lang_metadata() {
        let det = LanguageDetector::new();
        let doc = make_doc("This is a simple English sentence for language detection purposes.");
        let result = det.process(doc).await.unwrap();
        assert!(result.metadata.contains_key("lang"));
        assert_eq!(result.metadata.get("lang").map(|s| s.as_str()), Some("eng"));
    }

    #[tokio::test]
    async fn test_language_detector_passes_content_unchanged() {
        let det = LanguageDetector::new();
        let doc = make_doc("unchanged content");
        let result = det.process(doc).await.unwrap();
        assert_eq!(result.content, b"unchanged content");
    }
}
