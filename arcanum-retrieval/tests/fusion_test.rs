use arcanum_retrieval::RrfFusion;
use arcanum_core::types::*;

fn make_retrieved_for_doc(
    doc_id: DocumentId,
    text: &str,
    strategy: RetrievalStrategy,
) -> RetrievedChunk {
    RetrievedChunk {
        indexed_chunk: IndexedChunk {
            chunk: Chunk {
                id: ChunkId::new(),            // fresh — does NOT match across backends
                document_id: doc_id,           // shared — is the fusion key
                collection_id: CollectionId("test".into()),
                text: text.to_string(),
                position: ChunkPosition { start: 0, end: text.len(), index: 0 },
                metadata: ChunkMetadata::default(),
            },
            vector: Vector(vec![0.1]), token_vectors: None, store_id: String::new(),
        },
        score: 0.5, strategy,
    }
}

#[test]
fn test_rrf_fusion_merges_results() {
    let shared_doc_id = DocumentId::new();
    let strategy_results = vec![
        (RetrievalStrategy::Vector, vec![
            make_retrieved_for_doc(shared_doc_id.clone(), "rust is fast", RetrievalStrategy::Vector),
            make_retrieved_for_doc(DocumentId::new(), "python is easy", RetrievalStrategy::Vector),
        ]),
        (RetrievalStrategy::Bm25, vec![
            // Same document_id, different chunk_id (simulates per-backend chunking)
            make_retrieved_for_doc(shared_doc_id.clone(), "rust is fast", RetrievalStrategy::Bm25),
        ]),
    ];
    let fused = RrfFusion::fuse(strategy_results, 60.0);
    assert!(!fused.is_empty());
    // Document with shared_doc_id appears in both strategies — higher fused score, ranks first
    assert_eq!(fused[0].indexed_chunk.chunk.text, "rust is fast");
}

#[test]
fn document_appearing_in_two_strategies_ranks_above_single_strategy_document() {
    let shared_doc_id = DocumentId::new();
    let single_doc_id = DocumentId::new();

    let strategy_results = vec![
        (RetrievalStrategy::Vector, vec![
            // single_doc_id only appears here (score 0.9 — ranked first in Vector)
            make_retrieved_for_doc(single_doc_id.clone(), "only in vector", RetrievalStrategy::Vector),
            // shared_doc_id appears here (score 0.5 — ranked second in Vector)
            make_retrieved_for_doc(shared_doc_id.clone(), "in both vector and graph", RetrievalStrategy::Vector),
        ]),
        (RetrievalStrategy::Graph, vec![
            // shared_doc_id appears here too — gets RRF boost
            make_retrieved_for_doc(shared_doc_id.clone(), "in both vector and graph", RetrievalStrategy::Graph),
        ]),
    ];
    let fused = RrfFusion::fuse(strategy_results, 60.0);

    assert_eq!(fused.len(), 2);
    let first_doc_id = &fused[0].indexed_chunk.chunk.document_id;
    assert_eq!(*first_doc_id, shared_doc_id,
        "document appearing in both strategies should rank first due to RRF boost");
}

#[test]
fn documents_from_independent_chunk_ids_still_merge_by_document_id() {
    let doc_id = DocumentId::new();
    // Two retrievers return chunks from the same document but with different ChunkIds
    // (as happens when vector and graph use different chunkers)
    let strategy_results = vec![
        (RetrievalStrategy::Vector, vec![
            make_retrieved_for_doc(doc_id.clone(), "vector chunk text", RetrievalStrategy::Vector),
        ]),
        (RetrievalStrategy::Graph, vec![
            make_retrieved_for_doc(doc_id.clone(), "graph chunk text — different split", RetrievalStrategy::Graph),
        ]),
    ];
    let fused = RrfFusion::fuse(strategy_results, 60.0);
    // The two chunks have different ChunkIds but the same DocumentId.
    // They should be merged into ONE result (not two).
    assert_eq!(fused.len(), 1, "same doc_id from two strategies should produce one fused result");
}
