use arcanum_core::{traits::{Preprocessor, TextEnricher}, types::{EnrichIntent, EnrichRequest, RawDocument}, Result};
use async_trait::async_trait;
use scraper::{Html, Selector};
use std::sync::Arc;
use tracing::instrument;

pub struct ImageCaptioner {
    enricher: Arc<dyn TextEnricher>,
}

impl ImageCaptioner {
    pub fn new(enricher: Arc<dyn TextEnricher>) -> Self { Self { enricher } }
}

#[async_trait]
impl Preprocessor for ImageCaptioner {
    #[instrument(skip(self, doc), fields(preprocessor = "image", content_len = doc.content.len()), err)]
    async fn process(&self, mut doc: RawDocument) -> Result<RawDocument> {
        if doc.mime_type != "text/html" && doc.mime_type != "application/xhtml+xml" {
            return Ok(doc);
        }
        let html_str = String::from_utf8_lossy(&doc.content).to_string();

        // Collect img attributes synchronously before any await points.
        let img_data: Vec<(String, String)> = {
            let parsed = Html::parse_document(&html_str);
            let img_sel = Selector::parse("img").unwrap();
            parsed.select(&img_sel)
                .map(|img| {
                    let alt = img.value().attr("alt").unwrap_or("").trim().to_string();
                    let src = img.value().attr("src").unwrap_or("").to_string();
                    (alt, src)
                })
                .collect()
        };

        let mut captions = Vec::new();
        for (alt, src) in img_data {
            let caption = if !alt.is_empty() {
                alt
            } else {
                let req = EnrichRequest {
                    text: src,
                    intent: EnrichIntent::Caption,
                    context: None,
                };
                match self.enricher.enrich(req).await {
                    Ok(enriched) => enriched.0,
                    Err(_) => continue,
                }
            };
            captions.push(caption);
        }

        if !captions.is_empty() {
            let combined = format!("{}\n\nImages: {}", html_str, captions.join(". "));
            doc.content = combined.into_bytes();
        }
        Ok(doc)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arcanum_core::{Result as ArcanumResult, types::EnrichedText};

    struct FakeCaptioner;

    #[async_trait::async_trait]
    impl TextEnricher for FakeCaptioner {
        async fn enrich(&self, req: EnrichRequest) -> ArcanumResult<EnrichedText> {
            Ok(EnrichedText(format!("caption of {}", req.text)))
        }
    }

    #[tokio::test]
    async fn test_image_captioner_uses_alt_text() {
        let captioner = ImageCaptioner::new(Arc::new(FakeCaptioner));
        let html = r#"<html><body><img src="photo.jpg" alt="A cat sitting"/><p>Some text</p></body></html>"#;
        let doc = RawDocument::for_test(html, "text/html");
        let result = captioner.process(doc).await.unwrap();
        let text = String::from_utf8(result.content).unwrap();
        assert!(text.contains("A cat sitting"));
    }

    #[tokio::test]
    async fn test_image_captioner_calls_enricher_when_no_alt() {
        let captioner = ImageCaptioner::new(Arc::new(FakeCaptioner));
        let html = r#"<html><body><img src="diagram.png"/></body></html>"#;
        let doc = RawDocument::for_test(html, "text/html");
        let result = captioner.process(doc).await.unwrap();
        let text = String::from_utf8(result.content).unwrap();
        assert!(text.contains("caption of diagram.png"));
    }

    #[tokio::test]
    async fn test_image_captioner_passes_through_non_html() {
        let captioner = ImageCaptioner::new(Arc::new(FakeCaptioner));
        let doc = RawDocument::for_test("plain text", "text/plain");
        let result = captioner.process(doc).await.unwrap();
        assert_eq!(result.content, b"plain text");
    }
}
