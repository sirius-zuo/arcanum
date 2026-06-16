use arcanum_core::{traits::Source, types::{CollectionId, DocumentId, DocumentVersion, RawDocument, Chunk, Vector}};

pub struct IngestionState {
    pub source:        Source,
    pub collection_id: CollectionId,
    pub doc:           Option<RawDocument>,
    pub chunks:        Vec<Chunk>,       // vector chunks (primary)
    pub graph_chunks:  Vec<Chunk>,       // graph backend chunks
    pub tree_chunks:   Vec<Chunk>,       // tree backend chunks
    pub vectors:       Vec<Vector>,      // embeddings for state.chunks
    pub tree_vectors:  Vec<Vector>,      // embeddings for state.tree_chunks

    // Set by load stage — original bytes before preprocess overwrites doc.content.
    pub raw_content:   Option<Vec<u8>>,
    // Set by preprocess stage — structured JSON from Docling; None for non-Docling formats.
    pub canonical_json: Option<serde_json::Value>,
    // Set by snapshot stage — populated once the snapshot is persisted.
    pub snapshot_document_id: Option<DocumentId>,
    pub snapshot_version_num: Option<u32>,
    pub snapshot_uri:         Option<String>,
    pub canonical_uri:        Option<String>,
    /// Built by make_snapshot_stage; consumed and written to DB by make_register_version_stage.
    pub pending_version: Option<DocumentVersion>,
}

impl IngestionState {
    pub fn new(source: Source, collection_id: CollectionId) -> Self {
        Self {
            source, collection_id, doc: None,
            chunks: vec![], graph_chunks: vec![], tree_chunks: vec![],
            vectors: vec![], tree_vectors: vec![],
            raw_content: None, canonical_json: None,
            snapshot_document_id: None, snapshot_version_num: None,
            snapshot_uri: None, canonical_uri: None, pending_version: None,
        }
    }
}
