use arcanum_retrieval::{RetrievalOrchestrator, OrchestratorMode, QueryTransformer, CitationGenerator};
use arcanum_core::{traits::*, types::*};
use async_trait::async_trait;
use std::sync::Arc;

struct StubRetriever(RetrievalStrategy);
#[async_trait]
impl Retriever for StubRetriever {
    async fn retrieve(&self, _query: &Query) -> arcanum_core::Result<Vec<RetrievedChunk>> {
        Ok(vec![RetrievedChunk {
            indexed_chunk: IndexedChunk {
                chunk: Chunk {
                    id: ChunkId::new(), text: format!("result from {:?}", self.0),
                    document_id: DocumentId::new(),
                    collection_id: CollectionId("t".into()),
                    position: ChunkPosition { start: 0, end: 0, index: 0 },
                    metadata: ChunkMetadata::default(),
                    provenance: Default::default(),
                },
                vector: Vector(vec![]), token_vectors: None, store_id: "".into(),
            },
            score: 0.9, strategy: self.0.clone(),
        }])
    }
    fn strategy(&self) -> RetrievalStrategy { self.0.clone() }
}

#[tokio::test]
async fn test_mode_c_runs_all_strategies() {
    let orch = RetrievalOrchestrator::new(OrchestratorMode::ParallelFusion)
        .add_retriever(Arc::new(StubRetriever(RetrievalStrategy::Vector)))
        .add_retriever(Arc::new(StubRetriever(RetrievalStrategy::Bm25)));
    let results = orch.retrieve(&Query::new("test")).await.unwrap();
    assert!(results.chunks.len() >= 1);
}

/// Returns one chunk per retrieve() call, with a distinct document_id derived
/// from the query text — lets tests prove a retriever was actually invoked
/// once per transformed query, not just once for the original query.
struct QueryEchoingRetriever;
#[async_trait]
impl Retriever for QueryEchoingRetriever {
    async fn retrieve(&self, query: &Query) -> arcanum_core::Result<Vec<RetrievedChunk>> {
        let doc_id = DocumentId(uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_OID, query.text.as_bytes()));
        Ok(vec![RetrievedChunk {
            indexed_chunk: IndexedChunk {
                chunk: Chunk {
                    id: ChunkId::new(), text: query.text.clone(),
                    document_id: doc_id,
                    collection_id: CollectionId("t".into()),
                    position: ChunkPosition { start: 0, end: 0, index: 0 },
                    metadata: ChunkMetadata::default(),
                    provenance: Default::default(),
                },
                vector: Vector(vec![]), token_vectors: None, store_id: "".into(),
            },
            score: 0.9, strategy: RetrievalStrategy::Vector,
        }])
    }
    fn strategy(&self) -> RetrievalStrategy { RetrievalStrategy::Vector }
}

struct TwoVariantTransformer;
#[async_trait]
impl QueryTransformer for TwoVariantTransformer {
    async fn transform(&self, query: Query) -> arcanum_core::Result<Vec<Query>> {
        Ok(vec![
            Query { text: format!("{}-a", query.text), ..query.clone() },
            Query { text: format!("{}-b", query.text), ..query },
        ])
    }
}

#[tokio::test]
async fn test_retrieve_uses_query_transformer_to_fan_out_and_merge() {
    let orch = RetrievalOrchestrator::new(OrchestratorMode::ParallelFusion)
        .add_retriever(Arc::new(QueryEchoingRetriever))
        .with_query_transformer(Arc::new(TwoVariantTransformer));
    let results = orch.retrieve(&Query::new("test")).await.unwrap();
    let texts: std::collections::HashSet<&str> = results.chunks.iter()
        .map(|c| c.indexed_chunk.chunk.text.as_str()).collect();
    assert_eq!(texts.len(), 2, "should have retrieved once per transformed query variant");
    assert!(texts.contains("test-a"));
    assert!(texts.contains("test-b"));
}

#[tokio::test]
async fn test_retrieve_without_transformer_uses_original_query_only() {
    let orch = RetrievalOrchestrator::new(OrchestratorMode::ParallelFusion)
        .add_retriever(Arc::new(QueryEchoingRetriever));
    let results = orch.retrieve(&Query::new("test")).await.unwrap();
    assert_eq!(results.chunks.len(), 1);
    assert_eq!(results.chunks[0].indexed_chunk.chunk.text, "test");
}

struct KeepFirstOnlyReranker;
#[async_trait]
impl Reranker for KeepFirstOnlyReranker {
    async fn rerank(&self, _query: &Query, chunks: Vec<RetrievedChunk>) -> arcanum_core::Result<Vec<RetrievedChunk>> {
        Ok(chunks.into_iter().take(1).collect())
    }
}

#[tokio::test]
async fn test_retrieve_applies_configured_reranker() {
    let orch = RetrievalOrchestrator::new(OrchestratorMode::ParallelFusion)
        .add_retriever(Arc::new(StubRetriever(RetrievalStrategy::Vector)))
        .add_retriever(Arc::new(StubRetriever(RetrievalStrategy::Bm25)))
        .with_reranker(Arc::new(KeepFirstOnlyReranker));
    let results = orch.retrieve(&Query::new("test")).await.unwrap();
    assert_eq!(results.chunks.len(), 1, "reranker output should be used, not bypassed");
}

fn make_dupeable_retriever(doc_id: DocumentId, strategy: RetrievalStrategy) -> Arc<dyn Retriever> {
    struct FixedTextRetriever(DocumentId, RetrievalStrategy);
    #[async_trait]
    impl Retriever for FixedTextRetriever {
        async fn retrieve(&self, _query: &Query) -> arcanum_core::Result<Vec<RetrievedChunk>> {
            Ok(vec![RetrievedChunk {
                indexed_chunk: IndexedChunk {
                    chunk: Chunk {
                        id: ChunkId::new(), text: "duplicate boilerplate text".into(),
                        document_id: self.0.clone(),
                        collection_id: CollectionId("t".into()),
                        position: ChunkPosition { start: 0, end: 0, index: 0 },
                        metadata: ChunkMetadata::default(),
                        provenance: Default::default(),
                    },
                    vector: Vector(vec![]), token_vectors: None, store_id: "".into(),
                },
                score: 0.9, strategy: self.1.clone(),
            }])
        }
        fn strategy(&self) -> RetrievalStrategy { self.1.clone() }
    }
    Arc::new(FixedTextRetriever(doc_id, strategy))
}

#[tokio::test]
async fn test_retrieve_applies_dedup_threshold_across_documents() {
    // Two different documents, identical text — RrfFusion keys by document_id
    // so it alone won't collapse these; only Deduplicator's text/cosine check will.
    let orch = RetrievalOrchestrator::new(OrchestratorMode::ParallelFusion)
        .add_retriever(make_dupeable_retriever(DocumentId::new(), RetrievalStrategy::Vector))
        .add_retriever(make_dupeable_retriever(DocumentId::new(), RetrievalStrategy::Bm25))
        .with_dedup_threshold(1.0);
    let results = orch.retrieve(&Query::new("test")).await.unwrap();
    assert_eq!(results.chunks.len(), 1, "near-duplicate text across documents should be deduped");
}

#[tokio::test]
async fn test_retrieve_without_dedup_threshold_keeps_cross_document_duplicates() {
    let orch = RetrievalOrchestrator::new(OrchestratorMode::ParallelFusion)
        .add_retriever(make_dupeable_retriever(DocumentId::new(), RetrievalStrategy::Vector))
        .add_retriever(make_dupeable_retriever(DocumentId::new(), RetrievalStrategy::Bm25));
    let results = orch.retrieve(&Query::new("test")).await.unwrap();
    assert_eq!(results.chunks.len(), 2, "without a configured threshold, dedup should be skipped");
}

#[tokio::test]
async fn test_retrieve_populates_citations() {
    let orch = RetrievalOrchestrator::new(OrchestratorMode::ParallelFusion)
        .add_retriever(Arc::new(StubRetriever(RetrievalStrategy::Vector)));
    let results = orch.retrieve(&Query::new("test")).await.unwrap();
    assert_eq!(results.citations.len(), results.chunks.len(),
        "citations should be generated for every returned chunk");
    // Sanity-check against the standalone generator so this doesn't just assert a length.
    let expected = CitationGenerator::generate(&results.chunks);
    assert_eq!(results.citations[0].chunk_index, expected[0].chunk_index);
}
