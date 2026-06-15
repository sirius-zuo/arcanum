use arcanum_core::types::*;
use tracing::instrument;

// ---------------------------------------------------------------------------
// Deduplicator
// ---------------------------------------------------------------------------

/// Removes duplicate or near-duplicate chunks from a result set.
///
/// Two chunks are considered duplicates if their cosine similarity ≥ `threshold`.
/// When duplicates are found the one that appears first (highest score) is kept.
/// Chunks with empty vectors fall back to exact text deduplication.
pub struct Deduplicator;

impl Deduplicator {
    /// Deduplicate `chunks` using cosine similarity on their stored vectors.
    ///
    /// Chunks are expected to be in descending score order (best first); the
    /// first chunk encountered for a near-duplicate group is retained.
    #[instrument(fields(input_count = chunks.len(), output_count, threshold))]
    pub fn deduplicate(chunks: Vec<RetrievedChunk>, threshold: f32) -> Vec<RetrievedChunk> {
        let mut kept: Vec<RetrievedChunk> = Vec::with_capacity(chunks.len());

        'outer: for candidate in chunks {
            let cv = &candidate.indexed_chunk.vector.0;
            for existing in &kept {
                let ev = &existing.indexed_chunk.vector.0;
                let sim = if cv.is_empty() || ev.is_empty() {
                    // Fallback: exact text match counts as duplicate.
                    if candidate.indexed_chunk.chunk.text == existing.indexed_chunk.chunk.text {
                        1.0
                    } else {
                        0.0
                    }
                } else {
                    cosine(cv, ev)
                };
                if sim >= threshold {
                    continue 'outer; // drop duplicate
                }
            }
            kept.push(candidate);
        }
        let result = kept;
        tracing::Span::current().record("output_count", result.len());
        result
    }
}

fn cosine(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() { return 0.0; }
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let na: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let nb: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if na == 0.0 || nb == 0.0 { 0.0 } else { dot / (na * nb) }
}

// ---------------------------------------------------------------------------
// CitationGenerator
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct Citation {
    pub chunk_id: String,
    pub source_uri: String,
    pub title: Option<String>,
    pub section: Option<String>,
    pub chunk_index: usize,
    pub collection_id: String,
    pub version: Option<u32>,
    pub snapshot_uri: Option<String>,
}

/// Generates citations from a slice of retrieved chunks.
///
/// Metadata keys consulted (from `ChunkMetadata`):
/// - `"source_uri"` — document URI
/// - `"title"` — document title (optional)
/// - `"section"` — section heading (optional)
pub struct CitationGenerator;

impl CitationGenerator {
    /// Generates citations from a slice of retrieved chunks.
    ///
    /// Provenance-first: `source_uri`, `section`, `version`, and
    /// `snapshot_uri` are read from `ChunkProvenance`.  Metadata fields are
    /// kept as a fallback for legacy chunks that lack provenance.
    ///
    /// Metadata keys consulted (from `ChunkMetadata`):
    /// - `"source_uri"` — document URI (fallback)
    /// - `"title"` — document title (optional, fallback)
    ///
    /// Provenance fields (from `ChunkProvenance`):
    /// - `document_version` — document version number
    /// - `snapshot_uri` — raw snapshot location
    /// - `canonical_uri` — canonical version location (stored on citation as `snapshot_uri`)
    /// - `section` — section heading within the document
    #[instrument(fields(chunk_count = chunks.len(), citation_count))]
    pub fn generate(chunks: &[RetrievedChunk]) -> Vec<Citation> {
        let result: Vec<Citation> = chunks.iter().map(|c| {
            let chunk = &c.indexed_chunk.chunk;
            let provenance = &chunk.provenance;
            // Source URI: provenance if available, metadata as fallback.
            let source_uri = if provenance.source_uri.is_empty() {
                chunk.metadata.0.get("source_uri")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string()
            } else {
                provenance.source_uri.clone()
            };
            let title = chunk.metadata.0.get("title").and_then(|v| v.as_str()).map(str::to_string);
            let section = if let Some(ref s) = provenance.section {
                Some(s.clone())
            } else {
                chunk.metadata.0.get("section").and_then(|v| v.as_str()).map(str::to_string)
            };
            Citation {
                chunk_id: chunk.id.0.to_string(),
                source_uri,
                title,
                section,
                chunk_index: chunk.position.index,
                collection_id: chunk.collection_id.0.clone(),
                version: if provenance.document_version > 0 {
                    Some(provenance.document_version)
                } else {
                    None
                },
                snapshot_uri: if !provenance.snapshot_uri.is_empty() {
                    Some(provenance.snapshot_uri.clone())
                } else {
                    None
                },
            }
        }).collect();
        tracing::Span::current().record("citation_count", result.len());
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn make_chunk_with_text(text: &str, vector: Vec<f32>) -> RetrievedChunk {
        RetrievedChunk {
            indexed_chunk: IndexedChunk {
                chunk: Chunk {
                    id: ChunkId::new(),
                    text: text.to_string(),
                    document_id: DocumentId::new(),
                    collection_id: CollectionId("col".into()),
                    position: ChunkPosition { start: 0, end: text.len(), index: 0 },
                    metadata: ChunkMetadata(HashMap::from([
                        ("source_uri".to_string(), serde_json::json!("file://doc.pdf")),
                        ("title".to_string(), serde_json::json!("My Doc")),
                    ])),
                    provenance: arcanum_core::types::ChunkProvenance::default(),
                },
                vector: Vector(vector),
                token_vectors: None,
                store_id: "".into(),
            },
            score: 0.9,
            strategy: RetrievalStrategy::Vector,
        }
    }

    #[test]
    fn test_deduplicator_removes_exact_text_duplicates() {
        let a = make_chunk_with_text("hello world", vec![]);
        let b = make_chunk_with_text("hello world", vec![]); // exact duplicate
        let c = make_chunk_with_text("different text", vec![]);
        let result = Deduplicator::deduplicate(vec![a, b, c], 1.0);
        assert_eq!(result.len(), 2, "Exact text duplicate should be removed");
    }

    #[test]
    fn test_deduplicator_removes_cosine_near_duplicates() {
        let a = make_chunk_with_text("chunk a", vec![1.0, 0.0]);
        let b = make_chunk_with_text("chunk b", vec![1.0, 0.0]); // identical vector
        let c = make_chunk_with_text("chunk c", vec![0.0, 1.0]); // orthogonal
        let result = Deduplicator::deduplicate(vec![a, b, c], 0.99);
        assert_eq!(result.len(), 2, "Near-duplicate by cosine should be removed");
    }

    #[test]
    fn test_deduplicator_keeps_distinct_chunks() {
        let a = make_chunk_with_text("alpha", vec![1.0, 0.0]);
        let b = make_chunk_with_text("beta",  vec![0.0, 1.0]);
        let result = Deduplicator::deduplicate(vec![a, b], 0.99);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_citation_generator_extracts_metadata() {
        let chunk = make_chunk_with_text("test", vec![]);
        let citations = CitationGenerator::generate(&[chunk]);
        assert_eq!(citations.len(), 1);
        assert_eq!(citations[0].source_uri, "file://doc.pdf");
        assert_eq!(citations[0].title.as_deref(), Some("My Doc"));
        assert!(citations[0].section.is_none());
    }

    #[test]
    fn test_citation_generator_empty_input() {
        let citations = CitationGenerator::generate(&[]);
        assert!(citations.is_empty());
    }
}
