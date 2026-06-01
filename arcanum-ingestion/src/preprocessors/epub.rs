use arcanum_core::{traits::Preprocessor, types::*, Result, ArcanumError};
use async_trait::async_trait;
use std::collections::HashMap;
use std::io::{Cursor, Read};
use zip::ZipArchive;

pub struct EpubParser;

impl EpubParser {
    pub fn new() -> Self { Self }

    fn read_entry(zip: &mut ZipArchive<Cursor<Vec<u8>>>, path: &str) -> Result<String> {
        let mut entry = zip.by_name(path)
            .map_err(|e| ArcanumError::Ingestion(format!("EPUB: entry '{}' missing: {e}", path)))?;
        let mut buf = String::new();
        entry.read_to_string(&mut buf)
            .map_err(|e| ArcanumError::Ingestion(format!("EPUB: read '{}' failed: {e}", path)))?;
        Ok(buf)
    }

    /// Parses META-INF/container.xml to get the OPF file path.
    fn opf_path(zip: &mut ZipArchive<Cursor<Vec<u8>>>) -> Result<String> {
        let xml = Self::read_entry(zip, "META-INF/container.xml")?;
        let doc = roxmltree::Document::parse(&xml)
            .map_err(|e| ArcanumError::Ingestion(format!("container.xml parse error: {e}")))?;
        doc.descendants()
            .find(|n| n.tag_name().name() == "rootfile")
            .and_then(|n| n.attribute("full-path"))
            .map(str::to_string)
            .ok_or_else(|| ArcanumError::Ingestion("container.xml: no rootfile element".into()))
    }

    /// Parses the OPF file and returns spine hrefs in reading order.
    /// Skips items marked linear="no" (cover, TOC, etc.).
    fn spine_hrefs(opf_xml: &str) -> Result<Vec<String>> {
        let doc = roxmltree::Document::parse(opf_xml)
            .map_err(|e| ArcanumError::Ingestion(format!("OPF parse error: {e}")))?;

        let manifest: HashMap<String, String> = doc.descendants()
            .filter(|n| n.tag_name().name() == "item")
            .filter_map(|n| Some((n.attribute("id")?.to_string(), n.attribute("href")?.to_string())))
            .collect();

        let hrefs = doc.descendants()
            .filter(|n| n.tag_name().name() == "itemref")
            .filter(|n| n.attribute("linear").map_or(true, |v| v != "no"))
            .filter_map(|n| n.attribute("idref"))
            .filter_map(|idref| manifest.get(idref).cloned())
            .collect();

        Ok(hrefs)
    }

    /// Extracts the chapter heading (first h1/h2/h3) and clean body text from an XHTML file.
    fn extract_chapter(xhtml: &str) -> (Option<String>, String) {
        let html = scraper::Html::parse_document(xhtml);

        let title = ["h1", "h2", "h3"].iter().find_map(|tag| {
            let sel = scraper::Selector::parse(tag).ok()?;
            let raw = html.select(&sel).next()?.text().collect::<String>();
            let t = raw.split_whitespace().collect::<Vec<_>>().join(" ");
            if t.is_empty() { None } else { Some(t) }
        });

        // Prefer <body> to avoid <head> metadata leaking into output.
        let text = scraper::Selector::parse("body").ok()
            .and_then(|sel| html.select(&sel).next())
            .map(|body| body.text().collect::<String>())
            .unwrap_or_else(|| html.root_element().text().collect::<String>());

        let cleaned = text.split_whitespace().collect::<Vec<_>>().join(" ");
        (title, cleaned)
    }

    /// Resolves a manifest href (relative to the OPF directory) to a ZIP entry path.
    /// Strips any URL fragment (#anchor).
    fn resolve(opf_dir: &str, href: &str) -> String {
        let href = href.split('#').next().unwrap_or(href);
        if opf_dir.is_empty() { href.to_string() } else { format!("{opf_dir}/{href}") }
    }
}

#[async_trait]
impl Preprocessor for EpubParser {
    async fn process(&self, mut doc: RawDocument) -> Result<RawDocument> {
        if doc.mime_type != "application/epub+zip" { return Ok(doc); }

        let cursor = Cursor::new(doc.content.clone());
        let mut zip = ZipArchive::new(cursor)
            .map_err(|e| ArcanumError::Ingestion(format!("EPUB ZIP error: {e}")))?;

        let opf_path = Self::opf_path(&mut zip)?;
        let opf_dir = std::path::Path::new(&opf_path)
            .parent()
            .and_then(|p| p.to_str())
            .filter(|s| !s.is_empty())
            .unwrap_or("")
            .to_string();

        let opf_xml = Self::read_entry(&mut zip, &opf_path)?;
        let hrefs = Self::spine_hrefs(&opf_xml)?;

        let mut chapters: Vec<String> = Vec::new();
        for href in &hrefs {
            let path = Self::resolve(&opf_dir, href);
            let xhtml = match Self::read_entry(&mut zip, &path) {
                Ok(s) => s,
                Err(_) => continue,
            };
            let (title, text) = Self::extract_chapter(&xhtml);
            if text.is_empty() { continue; }
            let section = match title {
                Some(t) => format!("## {t}\n\n{text}"),
                None => text,
            };
            chapters.push(section);
        }

        doc.content = chapters.join("\n\n---\n\n").into_bytes();
        doc.mime_type = "text/plain".to_string();
        Ok(doc)
    }
}
