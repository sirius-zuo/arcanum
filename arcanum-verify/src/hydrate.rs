//! Join retrieved chunk records back into one source passage.

use arcanum_core::types::{ChunkId, ChunkMetadataRecord, DocumentId, Passage};
use std::collections::HashSet;

#[derive(Debug, Clone)]
pub struct HydratedPassage {
    pub ref_id: String,
    pub document_id: DocumentId,
    pub version_num: u32,
    pub source_uri: String,
    pub snapshot_uri: String,
    pub canonical_uri: Option<String>,
    pub section: Option<String>,
    pub page: Option<u32>,
    pub offset_start: usize,
    pub offset_end: usize,
    pub text: String,
    /// `(chunk id, start, end)` sorted by start.
    pub chunks: Vec<(ChunkId, usize, usize)>,
    pub version_status: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JoinError {
    MixedVersions,
    NotContiguous,
    TextMismatch,
}

pub fn join_chunks(
    ref_id: &str,
    records: Vec<ChunkMetadataRecord>,
) -> Result<HydratedPassage, JoinError> {
    let mut seen = HashSet::new();
    let mut recs: Vec<ChunkMetadataRecord> = records
        .into_iter()
        .filter(|r| seen.insert(r.chunk_id.clone()))
        .collect();
    recs.sort_by_key(|r| (r.offset_start, r.offset_end));
    let first = recs.first().ok_or(JoinError::NotContiguous)?;
    if recs
        .iter()
        .any(|r| r.document_id != first.document_id || r.version_num != first.version_num)
    {
        return Err(JoinError::MixedVersions);
    }
    let mut text = String::new();
    let mut cursor = first.offset_start;
    for r in &recs {
        if r.offset_end < r.offset_start || r.text.len() != r.offset_end - r.offset_start {
            return Err(JoinError::TextMismatch);
        }
        if r.offset_end <= cursor && !text.is_empty() {
            continue;
        }
        if r.offset_start > cursor {
            return Err(JoinError::NotContiguous);
        }
        let suffix = r
            .text
            .get(cursor - r.offset_start..)
            .ok_or(JoinError::TextMismatch)?;
        text.push_str(suffix);
        cursor = r.offset_end;
    }
    Ok(HydratedPassage {
        ref_id: ref_id.to_string(),
        document_id: first.document_id.clone(),
        version_num: first.version_num,
        source_uri: first.source_uri.clone(),
        snapshot_uri: first.snapshot_uri.clone(),
        canonical_uri: first.canonical_uri.clone(),
        section: first.section.clone(),
        page: first.page,
        offset_start: first.offset_start,
        offset_end: cursor,
        text,
        chunks: recs
            .iter()
            .map(|r| (r.chunk_id.clone(), r.offset_start, r.offset_end))
            .collect(),
        version_status: "unknown".to_string(),
    })
}

impl HydratedPassage {
    /// First chunk whose range contains `source_offset`, else the first chunk.
    pub fn chunk_at(&self, source_offset: usize) -> &ChunkId {
        self.chunks
            .iter()
            .find(|(_, s, e)| (*s..*e).contains(&source_offset))
            .map(|(id, _, _)| id)
            .unwrap_or(&self.chunks[0].0)
    }

    pub fn to_passage(&self) -> Passage {
        Passage {
            ref_id: self.ref_id.clone(),
            document_id: self.document_id.clone(),
            version_num: self.version_num,
            source_uri: self.source_uri.clone(),
            snapshot_uri: self.snapshot_uri.clone(),
            canonical_uri: self.canonical_uri.clone(),
            section: self.section.clone(),
            page: self.page,
            offset_start: self.offset_start,
            offset_end: self.offset_end,
            text: self.text.clone(),
            chunk_ids: self.chunks.iter().map(|(id, _, _)| id.clone()).collect(),
            strategies: vec![],
            score: 0.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arcanum_core::types::{ChunkBackend, ChunkId, ChunkMetadataRecord, DocumentId};

    const DOC: &str = "The quick brown fox jumps over the lazy dog.";

    fn rec(doc: &DocumentId, ver: u32, start: usize, end: usize) -> ChunkMetadataRecord {
        ChunkMetadataRecord {
            chunk_id: ChunkId::new(),
            document_id: doc.clone(),
            collection_id: "c".into(),
            version_num: ver,
            backend: ChunkBackend::Vector,
            text: DOC[start..end].to_string(),
            chunk_index: 0,
            source_uri: "raw://d".into(),
            snapshot_uri: "snap://d".into(),
            canonical_uri: None,
            page: None,
            section: None,
            block_ids: vec![],
            offset_start: start,
            offset_end: end,
            ingested_at: chrono::Utc::now(),
        }
    }

    #[test]
    fn join_overlap_out_of_order_and_duplicates() {
        let d = DocumentId::new();
        let a = rec(&d, 1, 0, 20);
        let b = rec(&d, 1, 15, 44);
        let inner = rec(&d, 1, 5, 10);
        let p = join_chunks("P1", vec![b.clone(), a.clone(), inner, a.clone()]).unwrap();
        assert_eq!(p.text, DOC);
        assert_eq!((p.offset_start, p.offset_end), (0, 44));
        assert_eq!(p.chunks.len(), 3);
        assert_eq!(p.chunk_at(10), &a.chunk_id);
        assert_eq!(p.chunk_at(30), &b.chunk_id);
        assert_eq!(p.version_status, "unknown");
    }

    #[test]
    fn join_rejects_gap_and_mixed_versions() {
        let d = DocumentId::new();
        assert!(matches!(
            join_chunks("P1", vec![rec(&d, 1, 0, 10), rec(&d, 1, 20, 30)]),
            Err(JoinError::NotContiguous)
        ));
        assert!(matches!(
            join_chunks("P1", vec![rec(&d, 1, 0, 20), rec(&d, 2, 15, 44)]),
            Err(JoinError::MixedVersions)
        ));
        assert!(join_chunks("P1", vec![rec(&d, 1, 0, 20), rec(&d, 1, 20, 44)]).is_ok());
    }

    #[test]
    fn non_char_boundary_is_text_mismatch() {
        let d = DocumentId::new();
        let mut a = rec(&d, 1, 0, 10);
        let mut b = rec(&d, 1, 9, 13);
        a.text = "abcdefghij".into();
        // cursor 10 lands one byte into the two-byte 'é'
        b.text = "éfg".into();
        assert!(matches!(
            join_chunks("P1", vec![a, b]),
            Err(JoinError::TextMismatch)
        ));
    }
}
