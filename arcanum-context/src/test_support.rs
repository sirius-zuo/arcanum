#![allow(dead_code)] // shared by later modules' tests

use arcanum_core::traits::TokenCounter;
use arcanum_core::types::*;

pub struct WordCounter;

impl TokenCounter for WordCounter {
    fn count(&self, text: &str) -> usize {
        text.split_whitespace().count()
    }
    fn name(&self) -> &'static str {
        "words"
    }
}

pub fn src(doc: &DocumentId, version: u32, doc_text: &str, start: usize, end: usize) -> Chunk {
    Chunk {
        id: ChunkId::new(),
        text: doc_text[start..end].to_string(),
        document_id: doc.clone(),
        collection_id: CollectionId("c".into()),
        position: ChunkPosition {
            start,
            end,
            index: 0,
        },
        metadata: ChunkMetadata::default(),
        provenance: ChunkProvenance {
            document_version: version,
            source_uri: format!("raw://{}", doc.0),
            ..Default::default()
        },
    }
}

fn retrieved(chunk: Chunk, strategy: RetrievalStrategy, kind: ChunkKind) -> RetrievedChunk {
    RetrievedChunk {
        indexed_chunk: IndexedChunk {
            chunk,
            vector: Vector(vec![]),
            token_vectors: None,
            store_id: "t".into(),
        },
        score: 0.0,
        strategy,
        kind,
    }
}

pub fn list(strategy: RetrievalStrategy, query_index: usize, chunks: Vec<Chunk>) -> CandidateList {
    let chunks = chunks
        .into_iter()
        .map(|c| retrieved(c, strategy.clone(), ChunkKind::Source))
        .collect();
    CandidateList {
        query_index,
        strategy,
        chunks,
    }
}

pub fn summary(doc: &DocumentId, level: u32, text: &str, covers: Vec<ChunkId>) -> RetrievedChunk {
    let mut chunk = src(doc, 1, text, 0, text.len());
    chunk.text = text.to_string();
    retrieved(
        chunk,
        RetrievalStrategy::Raptor,
        ChunkKind::Summary { level, covers },
    )
}
