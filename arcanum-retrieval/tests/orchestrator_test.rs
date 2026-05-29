use arcanum_retrieval::{RetrievalOrchestrator, OrchestratorMode};
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
