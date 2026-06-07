use arcanum_core::{traits::Source, types::{CollectionId, RawDocument, Chunk, Vector}};

pub struct IngestionState {
    pub source:        Source,
    pub collection_id: CollectionId,
    pub doc:           Option<RawDocument>,
    pub chunks:        Vec<Chunk>,       // vector chunks (primary)
    pub graph_chunks:  Vec<Chunk>,       // graph backend chunks
    pub tree_chunks:   Vec<Chunk>,       // tree backend chunks
    pub vectors:       Vec<Vector>,
}

impl IngestionState {
    pub fn new(source: Source, collection_id: CollectionId) -> Self {
        Self { source, collection_id, doc: None, chunks: vec![], graph_chunks: vec![], tree_chunks: vec![], vectors: vec![] }
    }
}
