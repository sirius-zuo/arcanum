use arcanum_retrieval::RrfFusion;
use arcanum_core::types::*;

fn make_retrieved(text: &str, strategy: RetrievalStrategy) -> RetrievedChunk {
    RetrievedChunk {
        indexed_chunk: IndexedChunk {
            chunk: Chunk {
                id: ChunkId::new(), text: text.to_string(),
                document_id: DocumentId::new(),
                collection_id: CollectionId("test".into()),
                position: ChunkPosition { start: 0, end: text.len(), index: 0 },
                metadata: ChunkMetadata::default(),
            },
            vector: Vector(vec![0.1]), token_vectors: None, store_id: String::new(),
        },
        score: 1.0, strategy,
    }
}

#[test]
fn test_rrf_fusion_merges_results() {
    let strategy_results = vec![
        (RetrievalStrategy::Vector, vec![
            make_retrieved("rust is fast", RetrievalStrategy::Vector),
            make_retrieved("python is easy", RetrievalStrategy::Vector),
        ]),
        (RetrievalStrategy::Bm25, vec![
            make_retrieved("rust is fast", RetrievalStrategy::Bm25),
        ]),
    ];
    let fused = RrfFusion::fuse(strategy_results, 60.0);
    assert!(!fused.is_empty());
    assert_eq!(fused[0].indexed_chunk.chunk.text, "rust is fast");
}
