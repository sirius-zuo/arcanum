use arcanum_core::{traits::Source, types::{CollectionId, RawDocument, Chunk, Vector}};

pub struct IngestionState {
    pub source:        Source,
    pub collection_id: CollectionId,
    pub doc:           Option<RawDocument>,
    pub chunks:        Vec<Chunk>,
    pub vectors:       Vec<Vector>,
}

impl IngestionState {
    pub fn new(source: Source, collection_id: CollectionId) -> Self {
        Self { source, collection_id, doc: None, chunks: vec![], vectors: vec![] }
    }
}
