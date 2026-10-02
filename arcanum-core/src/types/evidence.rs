use super::document::{Chunk, ChunkId, ChunkMetadata, ChunkPosition, CollectionId, DocumentId};
use super::provenance::ChunkProvenance;
use crate::ArcanumError;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum EvidenceKind {
    Chunk,
    TreeNode,
    Entity,
    Relation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProofNode {
    pub id: String,
    pub kind: EvidenceKind,
    pub label: String,
    pub metadata: serde_json::Value,
    pub children: Vec<ProofNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawSourceRef {
    pub document_id: DocumentId,
    pub version_num: u32,
    pub source_uri: String,
    pub snapshot_uri: String,
    pub canonical_uri: Option<String>,
    pub page: Option<u32>,
    pub section: Option<String>,
    pub block_ids: Vec<String>,
    pub offset_start: usize,
    pub offset_end: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProofChain {
    pub root: ProofNode,
    pub raw_sources: Vec<RawSourceRef>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChunkBackend {
    Vector,
    Lexical,
    Graph,
    Tree,
}

impl ChunkBackend {
    pub fn as_str(&self) -> &'static str {
        match self {
            ChunkBackend::Vector => "vector",
            ChunkBackend::Lexical => "lexical",
            ChunkBackend::Graph => "graph",
            ChunkBackend::Tree => "tree",
        }
    }
}

impl std::str::FromStr for ChunkBackend {
    type Err = ArcanumError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "vector" => Ok(ChunkBackend::Vector),
            "lexical" => Ok(ChunkBackend::Lexical),
            "graph" => Ok(ChunkBackend::Graph),
            "tree" => Ok(ChunkBackend::Tree),
            other => Err(ArcanumError::Storage(format!(
                "unknown chunk backend: {other}"
            ))),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkMetadataRecord {
    pub chunk_id: ChunkId,
    pub document_id: DocumentId,
    pub collection_id: String,
    pub version_num: u32,
    pub backend: ChunkBackend,
    pub text: String,
    pub chunk_index: usize,
    pub source_uri: String,
    pub snapshot_uri: String,
    pub canonical_uri: Option<String>,
    pub page: Option<u32>,
    pub section: Option<String>,
    pub block_ids: Vec<String>,
    pub offset_start: usize,
    pub offset_end: usize,
    pub ingested_at: DateTime<Utc>,
}

impl ChunkMetadataRecord {
    pub fn to_chunk(&self) -> Chunk {
        Chunk {
            id: self.chunk_id.clone(),
            text: self.text.clone(),
            document_id: self.document_id.clone(),
            collection_id: CollectionId(self.collection_id.clone()),
            position: ChunkPosition {
                start: self.offset_start,
                end: self.offset_end,
                index: self.chunk_index,
            },
            metadata: ChunkMetadata::default(),
            provenance: ChunkProvenance {
                document_version: self.version_num,
                source_uri: self.source_uri.clone(),
                snapshot_uri: self.snapshot_uri.clone(),
                canonical_uri: self.canonical_uri.clone(),
                page: self.page,
                section: self.section.clone(),
                block_ids: self.block_ids.clone(),
            },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GcReport {
    pub versions_deleted: u32,
    pub snapshots_removed: u32,
    pub chunks_removed: u32,
    pub errors: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proof_chain_roundtrips_json() {
        let doc_id = DocumentId::new();
        let chain = ProofChain {
            root: ProofNode {
                id: "abc".into(),
                kind: EvidenceKind::TreeNode,
                label: "summary of procurement policy".into(),
                metadata: serde_json::json!({}),
                children: vec![ProofNode {
                    id: "def".into(),
                    kind: EvidenceKind::Chunk,
                    label: "confluence://page/42 p.3".into(),
                    metadata: serde_json::json!({"page": 3}),
                    children: vec![],
                }],
            },
            raw_sources: vec![RawSourceRef {
                document_id: doc_id.clone(),
                version_num: 2,
                source_uri: "confluence://page/42".into(),
                snapshot_uri: "file:///data/snapshots/d/2.raw".into(),
                canonical_uri: None,
                page: Some(3),
                section: Some("§3.2".into()),
                block_ids: vec!["b1".into()],
                offset_start: 100,
                offset_end: 200,
            }],
        };
        let json = serde_json::to_string(&chain).unwrap();
        let back: ProofChain = serde_json::from_str(&json).unwrap();
        assert_eq!(back.raw_sources[0].version_num, 2);
        assert_eq!(back.root.children.len(), 1);
    }

    fn sample_record() -> ChunkMetadataRecord {
        ChunkMetadataRecord {
            chunk_id: ChunkId::new(),
            document_id: DocumentId::new(),
            collection_id: "col".into(),
            version_num: 4,
            backend: ChunkBackend::Lexical,
            text: "hello world".into(),
            chunk_index: 7,
            source_uri: "file://a.txt".into(),
            snapshot_uri: "file:///snap/a/4.raw".into(),
            canonical_uri: Some("https://a".into()),
            page: Some(2),
            section: Some("s".into()),
            block_ids: vec!["b1".into()],
            offset_start: 10,
            offset_end: 21,
            ingested_at: Utc::now(),
        }
    }

    #[test]
    fn to_chunk_maps_every_field() {
        let r = sample_record();
        let c = r.to_chunk();
        assert_eq!(c.id, r.chunk_id);
        assert_eq!(c.text, "hello world");
        assert_eq!(c.document_id, r.document_id);
        assert_eq!(c.collection_id.0, "col");
        assert_eq!(c.position.start, 10);
        assert_eq!(c.position.end, 21);
        assert_eq!(c.position.index, 7);
        assert!(c.metadata.0.is_empty());
        assert_eq!(c.provenance.document_version, 4);
        assert_eq!(c.provenance.source_uri, "file://a.txt");
        assert_eq!(c.provenance.snapshot_uri, "file:///snap/a/4.raw");
        assert_eq!(c.provenance.canonical_uri.as_deref(), Some("https://a"));
        assert_eq!(c.provenance.page, Some(2));
        assert_eq!(c.provenance.section.as_deref(), Some("s"));
        assert_eq!(c.provenance.block_ids, vec!["b1"]);
    }

    #[test]
    fn chunk_backend_serializes_lowercase() {
        assert_eq!(
            serde_json::to_string(&ChunkBackend::Lexical).unwrap(),
            "\"lexical\""
        );
        assert_eq!("tree".parse::<ChunkBackend>().unwrap(), ChunkBackend::Tree);
        assert_eq!(ChunkBackend::Graph.as_str(), "graph");
        assert!("bogus".parse::<ChunkBackend>().is_err());
    }

    #[test]
    fn gc_report_defaults_zero() {
        let r = GcReport {
            versions_deleted: 0,
            snapshots_removed: 0,
            chunks_removed: 0,
            errors: vec![],
        };
        assert_eq!(r.versions_deleted, 0);
    }
}
