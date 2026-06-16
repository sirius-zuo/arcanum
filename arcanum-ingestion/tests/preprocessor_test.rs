use arcanum_core::traits::Preprocessor;
use arcanum_core::types::*;
use arcanum_ingestion::{EpubParser, HtmlCleaner, PdfParser};
use std::io::{Cursor, Write};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

fn raw_doc(content: Vec<u8>, mime_type: &str) -> RawDocument {
    RawDocument {
        id: DocumentId::new(),
        content,
        mime_type: mime_type.into(),
        source_uri: "test".into(),
        metadata: Default::default(),
    }
}

// ── HtmlCleaner ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_html_cleaner_strips_tags() {
    let cleaner = HtmlCleaner::new();
    let doc = raw_doc(b"<h1>Title</h1><p>Hello <b>world</b></p>".to_vec(), "text/html");
    let out = cleaner.process(doc).await.unwrap();
    let text = String::from_utf8(out.content).unwrap();
    assert!(text.contains("Title"));
    assert!(text.contains("Hello"));
    assert!(!text.contains("<b>"));
}

#[tokio::test]
async fn test_html_cleaner_passthrough_non_html() {
    let cleaner = HtmlCleaner::new();
    let doc = raw_doc(b"plain text".to_vec(), "text/plain");
    let out = cleaner.process(doc).await.unwrap();
    assert_eq!(out.mime_type, "text/plain");
    assert_eq!(out.content, b"plain text");
}

// ── PdfParser ────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_pdf_passthrough_non_pdf() {
    let parser = PdfParser::new();
    let doc = raw_doc(b"not a pdf".to_vec(), "text/plain");
    let out = parser.process(doc).await.unwrap();
    assert_eq!(out.mime_type, "text/plain");
    assert_eq!(out.content, b"not a pdf");
}

#[tokio::test]
async fn test_pdf_invalid_bytes_returns_error() {
    let parser = PdfParser::new();
    let doc = raw_doc(b"this is not a pdf".to_vec(), "application/pdf");
    assert!(parser.process(doc).await.is_err());
}

#[tokio::test]
async fn test_pdf_valid_bytes_produce_plain_text_mime() {
    // Minimal valid PDF-1.4 with correct xref offsets; no content stream so
    // text extraction yields empty output, but mime_type must flip to text/plain.
    let minimal_pdf = b"\
%PDF-1.4\n\
1 0 obj<</Type/Catalog/Pages 2 0 R>>endobj\n\
2 0 obj<</Type/Pages/Kids[3 0 R]/Count 1>>endobj\n\
3 0 obj<</Type/Page/Parent 2 0 R/MediaBox[0 0 612 792]>>endobj\n\
xref\n\
0 4\n\
0000000000 65535 f \n\
0000000009 00000 n \n\
0000000052 00000 n \n\
0000000101 00000 n \n\
trailer<</Size 4/Root 1 0 R>>\n\
startxref\n\
164\n\
%%EOF";
    let parser = PdfParser::new();
    let doc = raw_doc(minimal_pdf.to_vec(), "application/pdf");
    let out = parser.process(doc).await.unwrap();
    assert_eq!(out.mime_type, "text/plain");
}

// ── EpubParser helpers ───────────────────────────────────────────────────────

struct EpubChapter<'a> {
    id: &'a str,
    heading: &'a str,
    body: &'a str,
    linear: bool,
}

impl<'a> EpubChapter<'a> {
    fn new(id: &'a str, heading: &'a str, body: &'a str) -> Self {
        Self { id, heading, body, linear: true }
    }
    fn nonlinear(mut self) -> Self {
        self.linear = false;
        self
    }
}

fn build_epub(chapters: &[EpubChapter<'_>]) -> Vec<u8> {
    let cursor = Cursor::new(Vec::new());
    let mut zip = ZipWriter::new(cursor);
    let stored = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    let deflated = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);

    zip.start_file("mimetype", stored).unwrap();
    zip.write_all(b"application/epub+zip").unwrap();

    zip.start_file("META-INF/container.xml", deflated).unwrap();
    zip.write_all(
        br#"<?xml version="1.0"?><container xmlns="urn:oasis:names:tc:opendocument:xmlns:container" version="1.0"><rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#,
    )
    .unwrap();

    let mut manifest = String::new();
    let mut spine = String::new();
    for ch in chapters {
        manifest.push_str(&format!(
            r#"<item id="{}" href="{}.xhtml" media-type="application/xhtml+xml"/>"#,
            ch.id, ch.id
        ));
        let linear_attr = if ch.linear { "" } else { r#" linear="no""# };
        spine.push_str(&format!(
            r#"<itemref idref="{}"{}/>"#,
            ch.id, linear_attr
        ));
    }
    let opf = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><package xmlns="http://www.idpf.org/2007/opf" version="3.0"><manifest>{manifest}</manifest><spine>{spine}</spine></package>"#
    );
    zip.start_file("OEBPS/content.opf", deflated).unwrap();
    zip.write_all(opf.as_bytes()).unwrap();

    for ch in chapters {
        let xhtml = format!(
            r#"<?xml version="1.0"?><html xmlns="http://www.w3.org/1999/xhtml"><head><title>{}</title></head><body><h1>{}</h1><p>{}</p></body></html>"#,
            ch.heading, ch.heading, ch.body
        );
        zip.start_file(format!("OEBPS/{}.xhtml", ch.id), deflated).unwrap();
        zip.write_all(xhtml.as_bytes()).unwrap();
    }

    zip.finish().unwrap().into_inner()
}

// ── EpubParser ───────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_epub_passthrough_non_epub() {
    let parser = EpubParser::new();
    let doc = raw_doc(b"not an epub".to_vec(), "text/plain");
    let out = parser.process(doc).await.unwrap();
    assert_eq!(out.mime_type, "text/plain");
    assert_eq!(out.content, b"not an epub");
}

#[tokio::test]
async fn test_epub_invalid_zip_returns_error() {
    let parser = EpubParser::new();
    let doc = raw_doc(b"not a zip".to_vec(), "application/epub+zip");
    assert!(parser.process(doc).await.is_err());
}

#[tokio::test]
async fn test_epub_extracts_two_chapters_in_spine_order() {
    let chapters = [
        EpubChapter::new("ch1", "Introduction", "Intro body text."),
        EpubChapter::new("ch2", "Chapter One", "Chapter one body text."),
    ];
    let parser = EpubParser::new();
    let doc = raw_doc(build_epub(&chapters), "application/epub+zip");
    let out = parser.process(doc).await.unwrap();
    assert_eq!(out.mime_type, "text/plain");
    let text = String::from_utf8(out.content).unwrap();

    assert!(text.contains("## Introduction"), "missing chapter 1 heading");
    assert!(text.contains("Intro body text."), "missing chapter 1 body");
    assert!(text.contains("## Chapter One"), "missing chapter 2 heading");
    assert!(text.contains("Chapter one body text."), "missing chapter 2 body");
    assert!(
        text.find("Introduction").unwrap() < text.find("Chapter One").unwrap(),
        "spine order not preserved"
    );
}

#[tokio::test]
async fn test_epub_heading_becomes_markdown_section() {
    let chapters = [EpubChapter::new("ch1", "My Heading", "Some content.")];
    let parser = EpubParser::new();
    let doc = raw_doc(build_epub(&chapters), "application/epub+zip");
    let out = parser.process(doc).await.unwrap();
    let text = String::from_utf8(out.content).unwrap();
    assert!(text.starts_with("## My Heading\n\n"), "heading not formatted as ## section");
}

#[tokio::test]
async fn test_epub_nonlinear_items_excluded() {
    let chapters = [
        EpubChapter::new("cover", "Cover Page", "Cover content.").nonlinear(),
        EpubChapter::new("ch1", "Chapter One", "Real content."),
    ];
    let parser = EpubParser::new();
    let doc = raw_doc(build_epub(&chapters), "application/epub+zip");
    let out = parser.process(doc).await.unwrap();
    let text = String::from_utf8(out.content).unwrap();
    assert!(!text.contains("Cover Page"), "non-linear item should be excluded");
    assert!(text.contains("Chapter One"), "linear item should be present");
}

#[tokio::test]
async fn test_epub_chapters_separated_by_horizontal_rule() {
    let chapters = [
        EpubChapter::new("ch1", "Part One", "First part."),
        EpubChapter::new("ch2", "Part Two", "Second part."),
    ];
    let parser = EpubParser::new();
    let doc = raw_doc(build_epub(&chapters), "application/epub+zip");
    let out = parser.process(doc).await.unwrap();
    let text = String::from_utf8(out.content).unwrap();
    assert!(text.contains("---"), "chapters must be separated by ---");
}

#[tokio::test]
async fn test_epub_output_mime_is_plain_text() {
    let chapters = [EpubChapter::new("ch1", "Title", "Body.")];
    let parser = EpubParser::new();
    let doc = raw_doc(build_epub(&chapters), "application/epub+zip");
    let out = parser.process(doc).await.unwrap();
    assert_eq!(out.mime_type, "text/plain");
}

// ── PreprocessorRegistry ─────────────────────────────────────────────────────

use arcanum_ingestion::PreprocessorRegistry;
use std::sync::Arc;

#[tokio::test]
async fn test_registry_runs_chain_for_matching_mime() {
    let registry = PreprocessorRegistry::new()
        .register("text/html", Arc::new(HtmlCleaner::new()));
    let doc = raw_doc(b"<p>hello</p>".to_vec(), "text/html");
    let out = registry.process(doc).await.unwrap();
    assert!(!String::from_utf8(out.content).unwrap().contains('<'));
}

#[tokio::test]
async fn test_registry_passthrough_unknown_mime() {
    let registry = PreprocessorRegistry::new();
    let doc = raw_doc(b"raw bytes".to_vec(), "application/octet-stream");
    let out = registry.process(doc).await.unwrap();
    assert_eq!(out.content, b"raw bytes");
}

#[tokio::test]
async fn test_registry_chain_runs_in_order() {
    use std::sync::{Arc as StdArc, Mutex};
    use arcanum_core::traits::Preprocessor;
    use arcanum_core::types::RawDocument;

    let log = StdArc::new(Mutex::new(vec![]));

    struct Recorder(StdArc<Mutex<Vec<u8>>>, u8);
    #[async_trait::async_trait]
    impl Preprocessor for Recorder {
        async fn process(&self, doc: RawDocument) -> arcanum_core::Result<RawDocument> {
            self.0.lock().unwrap().push(self.1);
            Ok(doc)
        }
        fn canonical(&self, _doc_id: &DocumentId) -> Option<serde_json::Value> {
            None
        }
        fn set_canonical(&self, _doc_id: &DocumentId, _canonical: serde_json::Value) {
        }
    }

    let registry = PreprocessorRegistry::new()
        .register("text/plain", Arc::new(Recorder(log.clone(), 1)))
        .register("text/plain", Arc::new(Recorder(log.clone(), 2)));

    let doc = raw_doc(b"data".to_vec(), "text/plain");
    registry.process(doc).await.unwrap();
    assert_eq!(*log.lock().unwrap(), vec![1u8, 2u8]);
}

#[tokio::test]
async fn test_default_chains_routes_html() {
    let registry = PreprocessorRegistry::default_chains();
    let doc = raw_doc(b"<h1>Title</h1><p>Body</p>".to_vec(), "text/html");
    let out = registry.process(doc).await.unwrap();
    assert_eq!(out.mime_type, "text/plain");
}

// ── DoclingPreprocessor + docling_chains ─────────────────────────────────────

use arcanum_ingestion::{DoclingPreprocessor, DoclingBackend};

#[tokio::test]
async fn test_docling_chains_passes_through_unsupported_mime() {
    let preprocessor = Arc::new(DoclingPreprocessor::new(DoclingBackend::Http {
        base_url: "http://localhost:9999".into(), // won't be called
        api_key: None,
        timeout_secs: 5,
        use_async: false,
        poll_interval_ms: 2000,
    }));
    let registry = PreprocessorRegistry::docling_chains(preprocessor);
    let doc = raw_doc(b"plain text".to_vec(), "text/plain");
    let out = registry.process(doc).await.unwrap();
    assert_eq!(out.mime_type, "text/plain");
    assert_eq!(out.content, b"plain text");
}

#[tokio::test]
async fn test_docling_chains_registered_for_pdf() {
    use wiremock::{MockServer, Mock, ResponseTemplate};
    use wiremock::matchers::{method, path};

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/convert/file"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "document": { "md_content": "# PDF content" },
            "status": "success"
        })))
        .mount(&server)
        .await;

    let preprocessor = Arc::new(DoclingPreprocessor::new(DoclingBackend::Http {
        base_url: server.uri(),
        api_key: None,
        timeout_secs: 10,
        use_async: false,
        poll_interval_ms: 2000,
    }));
    let registry = PreprocessorRegistry::docling_chains(preprocessor);
    let doc = raw_doc(b"%PDF-1.4".to_vec(), "application/pdf");
    let out = registry.process(doc).await.unwrap();
    assert_eq!(out.mime_type, "text/markdown");
    assert!(String::from_utf8(out.content).unwrap().contains("PDF content"));
}
