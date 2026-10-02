//! Shared chunk registration: builds `ChunkMetadataRecord`s from chunker output and writes
//! them to the registry. Every backend write stage uses this so that registry text is always
//! the source slice the chunker's offsets point at.

use crate::IngestionState;
use arcanum_core::{
    traits::ChunkMetadataStore,
    types::{Chunk, ChunkBackend, ChunkMetadataRecord, DocumentId},
    ArcanumError, Result,
};
use tokio::sync::Mutex;

fn register_err(message: String) -> ArcanumError {
    ArcanumError::Pipeline {
        stage: "register".into(),
        message,
    }
}

/// Build registry records for `chunks`. `text` is `doc_text[start..end]` (never `chunk.text`,
/// which enrichment may have rewritten), so offsets must be valid byte ranges on char
/// boundaries.
pub(crate) fn build_chunk_records(
    chunks: &[Chunk],
    backend: ChunkBackend,
    document_id: &DocumentId,
    version_num: u32,
    doc_text: &str,
) -> Result<Vec<ChunkMetadataRecord>> {
    chunks
        .iter()
        .map(|chunk| {
            let (start, end) = (chunk.position.start, chunk.position.end);
            if start > end || end > doc_text.len() {
                return Err(register_err(format!(
                    "chunk {} has invalid offsets {}..{} for document of {} bytes",
                    chunk.id.0,
                    start,
                    end,
                    doc_text.len()
                )));
            }
            let text = doc_text.get(start..end).ok_or_else(|| {
                register_err(format!(
                    "chunk {} offsets {}..{} are not char boundaries",
                    chunk.id.0, start, end
                ))
            })?;
            Ok(ChunkMetadataRecord {
                chunk_id: chunk.id.clone(),
                document_id: document_id.clone(),
                collection_id: chunk.collection_id.0.clone(),
                version_num,
                backend,
                text: text.to_string(),
                chunk_index: chunk.position.index,
                source_uri: chunk.provenance.source_uri.clone(),
                snapshot_uri: chunk.provenance.snapshot_uri.clone(),
                canonical_uri: chunk.provenance.canonical_uri.clone(),
                page: chunk.provenance.page,
                section: chunk.provenance.section.clone(),
                block_ids: chunk.provenance.block_ids.clone(),
                offset_start: start,
                offset_end: end,
                ingested_at: chrono::Utc::now(),
            })
        })
        .collect()
}

/// Write records sequentially; the first error is returned.
pub(crate) async fn register_chunks(
    store: &dyn ChunkMetadataStore,
    records: &[ChunkMetadataRecord],
) -> Result<()> {
    for record in records {
        store.put(record).await?;
    }
    Ok(())
}

/// Snapshot the state needed to register chunks: stable document id, version number and the
/// preprocessed document text.
pub(crate) async fn registration_inputs(
    state: &Mutex<IngestionState>,
    stage: &str,
) -> Result<(DocumentId, u32, String)> {
    let g = state.lock().await;
    let missing = |what: &str| ArcanumError::Pipeline {
        stage: stage.into(),
        message: format!("{what} not set"),
    };
    let doc_id = g
        .snapshot_document_id
        .clone()
        .ok_or_else(|| missing("snapshot_document_id"))?;
    let version = g
        .snapshot_version_num
        .ok_or_else(|| missing("snapshot_version_num"))?;
    let doc = g.doc.as_ref().ok_or_else(|| missing("doc"))?;
    Ok((
        doc_id,
        version,
        String::from_utf8_lossy(&doc.content).into_owned(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use arcanum_core::types::{ChunkId, ChunkMetadata, ChunkPosition, CollectionId};

    fn chunk(text: &str, start: usize, end: usize, index: usize) -> Chunk {
        Chunk {
            id: ChunkId::new(),
            text: text.into(),
            document_id: DocumentId::new(),
            collection_id: CollectionId("c".into()),
            position: ChunkPosition { start, end, index },
            metadata: ChunkMetadata::default(),
            provenance: Default::default(),
        }
    }

    #[test]
    fn build_chunk_records_uses_source_slice_not_chunk_text() {
        let doc_text = "alpha beta gamma";
        let c = chunk("CONTEXT: beta", 6, 10, 3);
        let doc_id = DocumentId::new();
        let recs = build_chunk_records(&[c], ChunkBackend::Vector, &doc_id, 2, doc_text).unwrap();
        assert_eq!(recs[0].text, "beta");
        assert_eq!(recs[0].backend, ChunkBackend::Vector);
        assert_eq!(recs[0].chunk_index, 3);
        assert_eq!(recs[0].version_num, 2);
        assert_eq!(recs[0].document_id, doc_id);
    }

    #[test]
    fn build_chunk_records_rejects_out_of_range_and_non_boundary_offsets() {
        let doc_id = DocumentId::new();
        let too_long = chunk("x", 0, 99, 0);
        assert!(
            build_chunk_records(&[too_long], ChunkBackend::Vector, &doc_id, 1, "short").is_err()
        );
        let inverted = chunk("x", 3, 1, 0);
        assert!(
            build_chunk_records(&[inverted], ChunkBackend::Vector, &doc_id, 1, "short").is_err()
        );
        // "é" is two bytes; offset 1 is inside it.
        let mid = chunk("x", 1, 2, 0);
        let err = build_chunk_records(&[mid], ChunkBackend::Vector, &doc_id, 1, "é!").unwrap_err();
        assert!(matches!(err, ArcanumError::Pipeline { ref stage, .. } if stage == "register"));
    }
}
