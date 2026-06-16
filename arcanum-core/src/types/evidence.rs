use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use super::document::{ChunkId, DocumentId};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum EvidenceKind {
    Chunk,
    TreeNode,
    Entity,
    Relation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProofNode {
    pub id:       String,
    pub kind:     EvidenceKind,
    pub label:    String,
    pub metadata: serde_json::Value,
    pub children: Vec<ProofNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawSourceRef {
    pub document_id:   DocumentId,
    pub version_num:   u32,
    pub source_uri:    String,
    pub snapshot_uri:  String,
    pub canonical_uri: Option<String>,
    pub page:          Option<u32>,
    pub section:       Option<String>,
    pub block_ids:     Vec<String>,
    pub offset_start:  usize,
    pub offset_end:    usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProofChain {
    pub root:        ProofNode,
    pub raw_sources: Vec<RawSourceRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkMetadataRecord {
    pub chunk_id:      ChunkId,
    pub document_id:   DocumentId,
    pub collection_id: String,
    pub version_num:   u32,
    pub source_uri:    String,
    pub snapshot_uri:  String,
    pub canonical_uri: Option<String>,
    pub page:          Option<u32>,
    pub section:       Option<String>,
    pub block_ids:     Vec<String>,
    pub offset_start:  usize,
    pub offset_end:    usize,
    pub ingested_at:   DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GcReport {
    pub versions_deleted:  u32,
    pub snapshots_removed: u32,
    pub chunks_removed:    u32,
    pub errors:            Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proof_chain_roundtrips_json() {
        let doc_id = DocumentId::new();
        let chain = ProofChain {
            root: ProofNode {
                id:       "abc".into(),
                kind:     EvidenceKind::TreeNode,
                label:    "summary of procurement policy".into(),
                metadata: serde_json::json!({}),
                children: vec![
                    ProofNode {
                        id:       "def".into(),
                        kind:     EvidenceKind::Chunk,
                        label:    "confluence://page/42 p.3".into(),
                        metadata: serde_json::json!({"page": 3}),
                        children: vec![],
                    }
                ],
            },
            raw_sources: vec![
                RawSourceRef {
                    document_id:   doc_id.clone(),
                    version_num:   2,
                    source_uri:    "confluence://page/42".into(),
                    snapshot_uri:  "file:///data/snapshots/d/2.raw".into(),
                    canonical_uri: None,
                    page:          Some(3),
                    section:       Some("§3.2".into()),
                    block_ids:     vec!["b1".into()],
                    offset_start:  100,
                    offset_end:    200,
                }
            ],
        };
        let json = serde_json::to_string(&chain).unwrap();
        let back: ProofChain = serde_json::from_str(&json).unwrap();
        assert_eq!(back.raw_sources[0].version_num, 2);
        assert_eq!(back.root.children.len(), 1);
    }

    #[test]
    fn gc_report_defaults_zero() {
        let r = GcReport { versions_deleted: 0, snapshots_removed: 0, chunks_removed: 0, errors: vec![] };
        assert_eq!(r.versions_deleted, 0);
    }
}
