pub struct MimeDetector;

impl MimeDetector {
    pub fn detect(content: &[u8], hint: Option<&str>) -> String {
        if let Some(kind) = infer::get(content) {
            let magic = kind.mime_type();
            if magic == "application/zip" {
                return Self::disambiguate_zip(content);
            }
            return magic.to_string();
        }
        hint.map(str::to_string)
            .unwrap_or_else(|| "application/octet-stream".to_string())
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
