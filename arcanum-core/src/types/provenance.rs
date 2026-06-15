use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use super::document::DocumentId;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChunkProvenance {
    pub document_version: u32,
    pub source_uri:       String,
    pub snapshot_uri:     String,
    pub canonical_uri:    Option<String>,
    pub page:             Option<u32>,
    pub section:          Option<String>,
    pub block_ids:        Vec<String>,
}

impl Default for ChunkProvenance {
    fn default() -> Self {
        Self {
            document_version: 0,
            source_uri:       String::new(),
            snapshot_uri:     String::new(),
            canonical_uri:    None,
            page:             None,
            section:          None,
            block_ids:        vec![],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum VersionStatus {
    Active,
    Superseded,
    Deleted,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum VersioningPolicy {
    Replace,
    AppendOnly,
    RetentionBased { days: u32 },
}

impl Default for VersioningPolicy {
    fn default() -> Self { Self::Replace }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentVersion {
    pub document_id:   DocumentId,
    pub version_num:   u32,
    pub source_uri:    String,
    pub collection_id: String,
    pub content_hash:  String,
    pub snapshot_uri:  String,
    pub canonical_uri: Option<String>,
    pub mime_type:     String,
    pub status:        VersionStatus,
    pub ingested_at:   DateTime<Utc>,
    pub extra:         HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone)]
pub struct SnapshotLocation {
    pub raw_uri:       String,
    pub canonical_uri: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunk_provenance_default_is_empty() {
        let p = ChunkProvenance::default();
        assert_eq!(p.document_version, 0);
        assert!(p.source_uri.is_empty());
        assert!(p.snapshot_uri.is_empty());
        assert!(p.canonical_uri.is_none());
        assert!(p.block_ids.is_empty());
    }

    #[test]
    fn chunk_provenance_roundtrips_json() {
        let p = ChunkProvenance {
            document_version: 3,
            source_uri:       "confluence://page/42".into(),
            snapshot_uri:     "file:///data/snapshots/abc/3.raw".into(),
            canonical_uri:    Some("file:///data/snapshots/abc/3.canonical.json".into()),
            page:             Some(7),
            section:          Some("2.1 > Overview".into()),
            block_ids:        vec!["b-007-a".into()],
        };
        let json = serde_json::to_string(&p).unwrap();
        let back: ChunkProvenance = serde_json::from_str(&json).unwrap();
        assert_eq!(p, back);
    }

    #[test]
    fn versioning_policy_default_is_replace() {
        assert_eq!(VersioningPolicy::default(), VersioningPolicy::Replace);
    }
}
