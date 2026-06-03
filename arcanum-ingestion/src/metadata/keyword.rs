use arcanum_core::types::RawDocument;
use std::collections::{HashMap, HashSet};
use tracing::instrument;

static STOP_WORDS: &[&str] = &[
    "a","an","the","is","are","was","were","be","been","have","has","had",
    "do","does","did","will","would","could","should","may","might","to",
    "of","in","on","at","by","for","with","as","from","this","that","it","its",
];

#[instrument(skip(doc), fields(doc_uri = %doc.source_uri, keyword_count))]
pub fn extract_keywords(doc: &RawDocument) -> HashMap<String, String> {
    let text = String::from_utf8_lossy(&doc.content).to_lowercase();
    let stops: HashSet<&str> = STOP_WORDS.iter().copied().collect();
    let mut freq: HashMap<String, usize> = HashMap::new();
    for word in text.split(|c: char| !c.is_alphabetic()) {
        if word.len() >= 4 && !stops.contains(word) {
            *freq.entry(word.to_string()).or_insert(0) += 1;
        }
    }
    let mut pairs: Vec<(String, usize)> = freq.into_iter().collect();
    pairs.sort_by(|a, b| b.1.cmp(&a.1));
    let keywords: Vec<String> = pairs.into_iter().take(10).map(|(k, _)| k).collect();
    if keywords.is_empty() { return HashMap::new(); }
    let mut meta = HashMap::new();
    meta.insert("keywords".to_string(), keywords.join(","));
    tracing::Span::current().record("keyword_count", keywords.len());
    meta
}

#[cfg(test)]
mod tests {
    use super::*;
    use arcanum_core::types::DocumentId;

    #[test]
    fn test_extract_keywords_returns_top_words() {
        let doc = RawDocument {
            id: DocumentId::new(),
            content: b"machine learning models learn from training data data data".to_vec(),
            mime_type: "text/plain".to_string(),
            source_uri: "test://x".to_string(),
            metadata: Default::default(),
        };
        let meta = extract_keywords(&doc);
        assert!(meta.get("keywords").unwrap().contains("data"));
    }
}
