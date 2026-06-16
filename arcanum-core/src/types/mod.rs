pub mod chunk_config;
pub mod document;
pub mod enrichment;
pub mod graph;
pub mod operation;
pub mod provenance;
pub mod query;
pub mod evidence;
pub mod tree;
pub use evidence::{EvidenceKind, ProofNode, RawSourceRef, ProofChain, ChunkMetadataRecord, GcReport};
pub use chunk_config::{
    ChunkStrategyConfig, PerBackendChunkConfig, PerBackendChunkers,
    ExperimentId, ShadowContext,
};
pub use document::*;
pub use enrichment::*;
pub use graph::*;
pub use operation::*;
pub use provenance::{ChunkProvenance, DocumentVersion, SnapshotLocation, VersionStatus, VersioningPolicy};
pub use query::*;
pub use tree::*;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IngestionStatus {
    Success,
    PartialSuccess { failed_stages: Vec<String> },
    Failed { stage_id: String, error: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StageResult {
    pub stage_id: String,
    pub success: bool,
    pub duration_ms: u64,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngestionReport {
    pub operation_id: OperationId,
    pub source_uri: String,
    pub pipeline_template: String,
    pub stage_results: Vec<StageResult>,
    pub total_chunks: usize,
    pub total_vectors: usize,
    pub document_fingerprint: String,
    pub status: IngestionStatus,
}

#[cfg(test)]
mod ingestion_report_tests {
    use super::*;
    #[test]
    fn test_ingestion_report_construction() {
        let report = IngestionReport {
            operation_id: OperationId::new(),
            source_uri: "file://test.pdf".to_string(),
            pipeline_template: "standard".to_string(),
            stage_results: vec![
                StageResult {
                    stage_id: "load".to_string(),
                    success: true,
                    duration_ms: 12,
                    error: None,
                },
            ],
            total_chunks: 5,
            total_vectors: 5,
            document_fingerprint: "abc123".to_string(),
            status: IngestionStatus::Success,
        };
        assert_eq!(report.total_chunks, 5);
        assert!(matches!(report.status, IngestionStatus::Success));
    }
}
