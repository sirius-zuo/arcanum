use crate::hydrate::hydrate;
use arcanum_core::{traits::*, types::*, Result};
use async_trait::async_trait;
use std::sync::Arc;
use tracing::instrument;

/// GraphRetriever: a `GraphPlanner` extracts seed entity names from the query,
/// a `GraphScorer` ranks the chunks those entities (and their neighbours) were
/// extracted from, and the winners are hydrated from the chunk metadata registry.
pub struct GraphRetriever {
    planner: Arc<dyn GraphPlanner>,
    graph_store: Arc<dyn GraphStore>,
    scorer: Arc<dyn GraphScorer>,
    chunk_metadata: Arc<dyn ChunkMetadataStore>,
}

impl GraphRetriever {
    pub fn new(
        planner: Arc<dyn GraphPlanner>,
        graph_store: Arc<dyn GraphStore>,
        scorer: Arc<dyn GraphScorer>,
        chunk_metadata: Arc<dyn ChunkMetadataStore>,
    ) -> Self {
        Self {
            planner,
            graph_store,
            scorer,
            chunk_metadata,
        }
    }
}

#[async_trait]
impl Retriever for GraphRetriever {
    #[instrument(skip(self), fields(strategy = "graph"), err)]
    async fn retrieve(&self, query: &Query) -> Result<Vec<RetrievedChunk>> {
        let collection_id = query.collection_id.as_ref().ok_or_else(|| {
            arcanum_core::ArcanumError::Config(
                "GraphRetriever requires an explicit collection_id".into(),
            )
        })?;
        let collection = collection_id.0.as_str();

        let seeds = self.planner.plan_entities(&query.text).await?;
        if seeds.is_empty() {
            return Ok(vec![]);
        }

        let hits = self
            .scorer
            .score(self.graph_store.as_ref(), collection, &seeds, query.top_k)
            .await?;
        hydrate(
            self.chunk_metadata.as_ref(),
            &hits,
            collection,
            RetrievalStrategy::Graph,
        )
        .await
    }

    fn strategy(&self) -> RetrievalStrategy {
        RetrievalStrategy::Graph
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arcanum_core::types::{EnrichRequest, EnrichedText};
    use arcanum_graph::{GraphQueryPlanner, InMemoryGraphStore};

    struct EmptyEnricher;
    #[async_trait::async_trait]
    impl arcanum_core::traits::TextEnricher for EmptyEnricher {
        async fn enrich(&self, _: EnrichRequest) -> arcanum_core::Result<EnrichedText> {
            Ok(EnrichedText(
                r#"{"entities":[],"relations":[]}"#.to_string(),
            ))
        }
    }

    struct FixedPlanner(Vec<String>);
    #[async_trait::async_trait]
    impl GraphPlanner for FixedPlanner {
        async fn plan_entities(&self, _query: &str) -> Result<Vec<String>> {
            Ok(self.0.clone())
        }
    }

    struct StubScorer(Vec<(ChunkId, f32)>);
    #[async_trait::async_trait]
    impl GraphScorer for StubScorer {
        async fn score(
            &self,
            _: &dyn GraphStore,
            _: &str,
            _: &[String],
            _: usize,
        ) -> Result<Vec<(ChunkId, f32)>> {
            Ok(self.0.clone())
        }
    }

    fn record(text: &str) -> ChunkMetadataRecord {
        ChunkMetadataRecord {
            chunk_id: ChunkId::new(),
            document_id: DocumentId::new(),
            collection_id: "col".into(),
            version_num: 1,
            backend: ChunkBackend::Graph,
            text: text.into(),
            chunk_index: 0,
            source_uri: "file://x.txt".into(),
            snapshot_uri: "file:///snap/x/1.raw".into(),
            canonical_uri: None,
            page: None,
            section: None,
            block_ids: vec![],
            offset_start: 0,
            offset_end: text.len(),
            ingested_at: chrono::Utc::now(),
        }
    }

    #[tokio::test]
    async fn graph_results_are_registry_chunks_ranked_by_scorer() {
        let (c1, c2) = (record("one"), record("two"));
        let store = Arc::new(InMemoryChunkMetadataStore::new());
        store.put(&c1).await.unwrap();
        store.put(&c2).await.unwrap();
        let retriever = GraphRetriever::new(
            Arc::new(FixedPlanner(vec!["Acme".into()])),
            Arc::new(InMemoryGraphStore::new()),
            Arc::new(StubScorer(vec![
                (c2.chunk_id.clone(), 0.9),
                (c1.chunk_id.clone(), 0.5),
            ])),
            store,
        );
        let query = Query::new("q").with_collection(CollectionId("col".into()));
        let results = retriever.retrieve(&query).await.unwrap();
        let ids: Vec<_> = results
            .iter()
            .map(|r| r.indexed_chunk.chunk.id.clone())
            .collect();
        assert_eq!(ids, vec![c2.chunk_id, c1.chunk_id]);
        for r in &results {
            assert_eq!(r.strategy, RetrievalStrategy::Graph);
            assert_eq!(r.kind, ChunkKind::Source);
        }
    }

    #[tokio::test]
    async fn no_seed_entities_returns_empty() {
        let retriever = GraphRetriever::new(
            Arc::new(GraphQueryPlanner::new(Arc::new(EmptyEnricher), 2)),
            Arc::new(InMemoryGraphStore::new()),
            Arc::new(StubScorer(vec![(ChunkId::new(), 1.0)])),
            Arc::new(InMemoryChunkMetadataStore::new()),
        );
        let query = Query::new("who is the CEO?").with_collection(CollectionId("col".into()));
        assert!(retriever.retrieve(&query).await.unwrap().is_empty());
        assert_eq!(retriever.strategy(), RetrievalStrategy::Graph);
    }
}
