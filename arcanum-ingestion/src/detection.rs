use tracing::instrument;

pub struct MimeDetector;

impl MimeDetector {
    #[instrument(skip(content), fields(content_len = content.len(), hint = ?hint, detected_mime), ret)]
    pub fn detect(content: &[u8], hint: Option<&str>) -> String {
        let result = if let Some(kind) = infer::get(content) {
            let magic = kind.mime_type();
            if magic == "application/zip" {
                Self::disambiguate_zip(content)
            } else {
                magic.to_string()
            }
        } else {
            hint.map(str::to_string)
                .unwrap_or_else(|| "application/octet-stream".to_string())
        };
        tracing::Span::current().record("detected_mime", &result as &str);
        result
    }

    fn disambiguate_zip(content: &[u8]) -> String {
        let cursor = std::io::Cursor::new(content);
        let mut zip = match zip::ZipArchive::new(cursor) {
            Ok(z) => z,
            Err(_) => return "application/zip".to_string(),
        };
        if zip.by_name("META-INF/container.xml").is_ok() {
            return "application/epub+zip".to_string();
        }
        if zip.by_name("[Content_Types].xml").is_ok() {
            return "application/vnd.openxmlformats".to_string();
        }
        "application/zip".to_string()
    }
}
