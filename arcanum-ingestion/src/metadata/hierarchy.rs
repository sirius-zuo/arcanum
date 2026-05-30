use arcanum_core::types::RawDocument;
use std::collections::HashMap;

pub fn extract_hierarchy(doc: &RawDocument) -> HashMap<String, String> {
    let text = String::from_utf8_lossy(&doc.content);
    let mut headings: Vec<String> = Vec::new();
    for line in text.lines() {
        if line.starts_with("### ") {
            headings.push(format!("H3:{}", line.trim_start_matches('#').trim()));
        } else if line.starts_with("## ") {
            headings.push(format!("H2:{}", line.trim_start_matches('#').trim()));
        } else if line.starts_with("# ") {
            headings.push(format!("H1:{}", line.trim_start_matches('#').trim()));
        }
    }
    if headings.is_empty() { return HashMap::new(); }
    let mut meta = HashMap::new();
    meta.insert("section_hierarchy".to_string(), headings.join("|"));
    meta
}

#[cfg(test)]
mod tests {
    use super::*;
    use arcanum_core::types::DocumentId;

    #[test]
    fn test_extract_hierarchy_captures_headings() {
        let doc = RawDocument {
            id: DocumentId::new(),
            content: b"# Chapter 1\n## Section 1.1\ntext".to_vec(),
            mime_type: "text/plain".to_string(),
            source_uri: "test://x.md".to_string(),
            metadata: Default::default(),
        };
        let meta = extract_hierarchy(&doc);
        assert!(meta.get("section_hierarchy").unwrap().contains("H1:Chapter 1"));
    }
}
