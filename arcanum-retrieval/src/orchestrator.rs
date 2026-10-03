use crate::fusion::RrfFusion;
use crate::processor::{CitationGenerator, Deduplicator};
use crate::reranker::NullReranker;
use crate::transformer::QueryTransformer;
use arcanum_core::{traits::*, types::*, Result};
use metrics;
use std::{sync::Arc, time::Duration};
use tracing::{instrument, Instrument};

pub enum OrchestratorMode {
    Static(Vec<RetrievalStrategy>),
    ParallelFusion,
    QueryClassified,
}

pub struct RetrievalOrchestrator {
    mode: OrchestratorMode,
    retrievers: Vec<Arc<dyn Retriever>>,
    strategy_timeout: Duration,
    query_transformer: Option<Arc<dyn QueryTransformer>>,
    reranker: Arc<dyn Reranker>,
    dedup_threshold: Option<f32>,
}

impl RetrievalOrchestrator {
    pub fn new(mode: OrchestratorMode) -> Self {
        Self {
            mode,
            retrievers: vec![],
            strategy_timeout: Duration::from_secs(5),
            query_transformer: None,
            reranker: Arc::new(NullReranker),
            dedup_threshold: None,
        }
    }

    pub fn add_retriever(mut self, r: Arc<dyn Retriever>) -> Self {
        self.retrievers.push(r);
        self
    }

    /// Fans a query out to N queries before retrieval (e.g. HyDE, multi-query
    /// rephrasing). Each resulting query is retrieved and fused independently,
    /// then the per-query results are merged via another RRF pass. Unset by
    /// default: `retrieve()` runs against the original query only.
    pub fn with_query_transformer(mut self, t: Arc<dyn QueryTransformer>) -> Self {
        self.query_transformer = Some(t);
        self
    }

    /// Reorders/rescoring pass applied after fusion. Defaults to `NullReranker`
    /// (passthrough), so unconfigured behavior is unchanged.
    pub fn with_reranker(mut self, r: Arc<dyn Reranker>) -> Self {
        self.reranker = r;
        self
    }

    /// Enables `Deduplicator` on the reranked result set, at the given cosine
    /// similarity threshold. Unset by default — dedup is skipped, matching
    /// prior behavior — since RRF fusion only dedupes by document_id and
    /// can't catch near-duplicate content across different documents.
    pub fn with_dedup_threshold(mut self, threshold: f32) -> Self {
        self.dedup_threshold = Some(threshold);
        self
    }

    #[instrument(skip(self), fields(mode = ?self.mode_name(), retriever_count = self.retrievers.len()), err)]
    pub async fn retrieve(&self, query: &Query) -> Result<RetrievalResult> {
        let queries = self.transform(query).await;

        let mut per_query_fused = Vec::with_capacity(queries.len());
        for q in &queries {
            let fused = RrfFusion::fuse(self.fan_out(q).await.0, 60.0);
            // The strategy tag here is unused by RrfFusion::fuse (it only
            // groups/scores by document_id) — it's a required tuple slot for
            // reusing the same fusion pass to merge per-query result sets.
            per_query_fused.push((RetrievalStrategy::Vector, fused));
        }

        let fused = if per_query_fused.len() == 1 {
            per_query_fused.into_iter().next().unwrap().1
        } else {
            RrfFusion::fuse(per_query_fused, 60.0)
        };
        let strategy_scores: std::collections::HashMap<String, f32> = fused
            .iter()
            .map(|c| (format!("{:?}", c.strategy), c.score))
            .collect();

        let reranked = match self.reranker.rerank(query, fused.clone()).await {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!(err = ?e, "reranker failed; falling back to fused order");
                fused
            }
        };

        let deduped = match self.dedup_threshold {
            Some(threshold) => Deduplicator::deduplicate(reranked, threshold),
            None => reranked,
        };

        let citations = CitationGenerator::generate(&deduped)
            .into_iter()
            .map(|c| Citation {
                document_uri: c.source_uri,
                document_title: c.title,
                section: c.section,
                chunk_index: c.chunk_index,
                version: c.version,
                snapshot_uri: c.snapshot_uri,
            })
            .collect();

        Ok(RetrievalResult {
            chunks: deduped,
            citations,
            strategy_scores,
            confidence: 0.8,
        })
    }

    /// Pre-fusion retrieval: one `CandidateList` per (transformed query,
    /// successful strategy), including successful empty lists. No fusion,
    /// rerank or dedup. `failed` holds each failing strategy once with the
    /// first reason seen.
    pub async fn retrieve_candidates(&self, query: &Query) -> Result<Candidates> {
        let queries = self.transform(query).await;
        let mut lists = Vec::new();
        let mut failed: Vec<(RetrievalStrategy, String)> = Vec::new();
        for (query_index, q) in queries.iter().enumerate() {
            let (results, fails) = self.fan_out(q).await;
            for (strategy, chunks) in results {
                lists.push(CandidateList {
                    query_index,
                    strategy,
                    chunks,
                });
            }
            for (strategy, reason) in fails {
                if !failed.iter().any(|(s, _)| *s == strategy) {
                    failed.push((strategy, reason));
                }
            }
        }
        Ok(Candidates {
            queries: queries.into_iter().map(|q| q.text).collect(),
            lists,
            failed,
        })
    }

    /// Applies the query transformer, falling back to the original query when
    /// it is unset, fails, or returns nothing.
    async fn transform(&self, query: &Query) -> Vec<Query> {
        match &self.query_transformer {
            Some(t) => match t.transform(query.clone()).await {
                Ok(qs) if !qs.is_empty() => qs,
                Ok(_) => {
                    tracing::warn!(
                        "query transformer returned no queries; falling back to original"
                    );
                    vec![query.clone()]
                }
                Err(e) => {
                    tracing::warn!(err = ?e, "query transformer failed; falling back to original query");
                    vec![query.clone()]
                }
            },
            None => vec![query.clone()],
        }
    }

    /// Runs every active retriever for `query` in parallel (with per-strategy
    /// timeout). Returns the per-strategy results plus the strategies that
    /// failed or timed out (reason is "timeout" or the error text). Failures
    /// are logged, never fail the whole call.
    async fn fan_out(
        &self,
        query: &Query,
    ) -> (
        Vec<(RetrievalStrategy, Vec<RetrievedChunk>)>,
        Vec<(RetrievalStrategy, String)>,
    ) {
        let active = self.active_retrievers(query);
        let tasks: Vec<_> = active.iter().map(|r| {
            let r = r.clone();
            let q = query.clone();
            let t = self.strategy_timeout;
            // Capture a child span for each retriever task so it appears under
            // the parent `retrieve` span even after tokio::spawn breaks the context.
            let span = tracing::info_span!(
                "retrieval.strategy",
                strategy = ?r.strategy(),
            );
            tokio::spawn(async move {
                let t_start = std::time::Instant::now();
                let result = tokio::time::timeout(t, r.retrieve(&q)).await;
                let strategy = r.strategy();
                let strategy_name = format!("{:?}", strategy);
                let elapsed = t_start.elapsed().as_secs_f64();
                let status = match &result {
                    Ok(Ok(_))  => "ok",
                    Ok(Err(_)) => "error",
                    Err(_)     => "timeout",
                };
                metrics::counter!("arcanum_retrieval_total",
                    "strategy" => strategy_name.clone(), "status" => status).increment(1);
                metrics::histogram!("arcanum_retrieval_duration_seconds",
                    "strategy" => strategy_name).record(elapsed);
                match result {
                    Ok(Ok(chunks)) => {
                        tracing::debug!(strategy = ?strategy, chunk_count = chunks.len(), "strategy succeeded");
                        Ok((strategy, chunks))
                    }
                    Ok(Err(e)) => {
                        tracing::warn!(strategy = ?strategy, err = ?e, "strategy failed");
                        Err((strategy, e.to_string()))
                    }
                    Err(_) => {
                        tracing::warn!(strategy = ?strategy, "strategy timed out");
                        Err((strategy, "timeout".to_string()))
                    }
                }
            }.instrument(span))
        }).collect();

        let mut strategy_results = vec![];
        let mut failed = vec![];
        for task in tasks {
            match task.await {
                Ok(Ok(r)) => strategy_results.push(r),
                Ok(Err(f)) => failed.push(f),
                Err(_) => {}
            }
        }

        (strategy_results, failed)
    }

    fn mode_name(&self) -> &'static str {
        match &self.mode {
            OrchestratorMode::Static(_) => "static",
            OrchestratorMode::ParallelFusion => "parallel_fusion",
            OrchestratorMode::QueryClassified => "query_classified",
        }
    }

    fn active_retrievers(&self, query: &Query) -> Vec<Arc<dyn Retriever>> {
        match &self.mode {
            OrchestratorMode::ParallelFusion => self.retrievers.clone(),
            OrchestratorMode::Static(strategies) => self
                .retrievers
                .iter()
                .filter(|r| strategies.contains(&r.strategy()))
                .cloned()
                .collect(),
            OrchestratorMode::QueryClassified => {
                let selected = classify_query(&query.text);
                self.retrievers
                    .iter()
                    .filter(|r| selected.contains(&r.strategy()))
                    .cloned()
                    .collect()
            }
        }
    }
}

/// Mode B query classifier.
///
/// Uses lightweight lexical heuristics to select retrieval strategies.
/// No model call — deterministic and sub-millisecond.
///
/// Rules:
/// - Raptor signals: summarise/overview intent → [Raptor]
/// - Graph signals: relational/entity queries, quoted strings, proper nouns → [Graph, Vector]
/// - Default: [Vector, Bm25]
pub fn classify_query(query: &str) -> Vec<RetrievalStrategy> {
    let lower = query.to_lowercase();

    // RAPTOR: document-level summarisation signals.
    let raptor_signals = [
        "summarize",
        "summarise",
        "overview",
        "across all",
        "throughout",
    ];
    if raptor_signals.iter().any(|s| lower.contains(s)) {
        return vec![RetrievalStrategy::Raptor];
    }

    // GRAPH: entity/relational signals.
    let graph_signals = ["who is", "ceo", "founder", "relationship between"];
    let has_quoted = query.contains('"') || query.contains('\'');
    let has_proper_nouns = count_proper_nouns(query) >= 2;

    if graph_signals.iter().any(|s| lower.contains(s)) || has_quoted || has_proper_nouns {
        return vec![RetrievalStrategy::Graph, RetrievalStrategy::Vector];
    }

    // Default: lexical + semantic.
    vec![RetrievalStrategy::Vector, RetrievalStrategy::Bm25]
}

/// Count words that start with an uppercase letter (simple proper-noun heuristic).
fn count_proper_nouns(query: &str) -> usize {
    query
        .split_whitespace()
        .filter(|w| w.chars().next().map(|c| c.is_uppercase()).unwrap_or(false))
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_entity_query_includes_graph() {
        let strategies = classify_query("who is the CEO of Anthropic?");
        assert!(
            strategies.contains(&RetrievalStrategy::Graph),
            "Entity query should include Graph"
        );
    }

    #[test]
    fn test_classify_summary_query_includes_raptor() {
        let strategies = classify_query("summarize the entire document");
        assert!(
            strategies.contains(&RetrievalStrategy::Raptor),
            "Summary query should include Raptor"
        );
    }

    #[test]
    fn test_classify_default_query_vector_bm25() {
        let strategies = classify_query("what is retrieval augmented generation?");
        assert!(strategies.contains(&RetrievalStrategy::Vector));
        assert!(strategies.contains(&RetrievalStrategy::Bm25));
        assert!(!strategies.contains(&RetrievalStrategy::Graph));
        assert!(!strategies.contains(&RetrievalStrategy::Raptor));
    }

    #[test]
    fn test_classify_proper_nouns_triggers_graph() {
        let strategies = classify_query("Sam Altman OpenAI mission");
        assert!(strategies.contains(&RetrievalStrategy::Graph));
    }

    #[test]
    fn test_classify_overview_triggers_raptor() {
        let strategies = classify_query("give me an overview of all chapters");
        assert!(strategies.contains(&RetrievalStrategy::Raptor));
    }

    use crate::transformer::QueryTransformer;
    use arcanum_core::types::{
        Chunk, ChunkId, ChunkKind, ChunkMetadata, ChunkPosition, DocumentId, IndexedChunk, Vector,
    };

    fn stub_chunk(strategy: RetrievalStrategy) -> RetrievedChunk {
        RetrievedChunk {
            indexed_chunk: IndexedChunk {
                chunk: Chunk {
                    id: ChunkId::new(),
                    text: "t".into(),
                    document_id: DocumentId::new(),
                    collection_id: CollectionId(String::new()),
                    position: ChunkPosition {
                        start: 0,
                        end: 1,
                        index: 0,
                    },
                    metadata: ChunkMetadata::default(),
                    provenance: Default::default(),
                },
                vector: Vector(vec![]),
                token_vectors: None,
                store_id: "s".into(),
            },
            score: 1.0,
            strategy,
            kind: ChunkKind::Source,
        }
    }

    enum Behavior {
        Hits(usize),
        Fail,
        Hang,
    }
    struct StubRetriever(RetrievalStrategy, Behavior);
    #[async_trait::async_trait]
    impl Retriever for StubRetriever {
        async fn retrieve(&self, _q: &Query) -> Result<Vec<RetrievedChunk>> {
            match &self.1 {
                Behavior::Hits(n) => Ok((0..*n).map(|_| stub_chunk(self.0.clone())).collect()),
                Behavior::Fail => Err(arcanum_core::ArcanumError::Retrieval("boom".into())),
                Behavior::Hang => {
                    tokio::time::sleep(Duration::from_secs(30)).await;
                    Ok(vec![])
                }
            }
        }
        fn strategy(&self) -> RetrievalStrategy {
            self.0.clone()
        }
    }

    struct TwoQueries;
    #[async_trait::async_trait]
    impl QueryTransformer for TwoQueries {
        async fn transform(&self, q: Query) -> Result<Vec<Query>> {
            Ok(vec![q.clone(), Query::new("second")])
        }
    }

    fn orch(b: Vec<(RetrievalStrategy, Behavior)>) -> RetrievalOrchestrator {
        let mut o = RetrievalOrchestrator::new(OrchestratorMode::ParallelFusion);
        for (s, beh) in b {
            o = o.add_retriever(Arc::new(StubRetriever(s, beh)));
        }
        o
    }

    #[tokio::test]
    async fn retrieve_candidates_one_list_per_query_and_strategy() {
        let o = orch(vec![
            (RetrievalStrategy::Vector, Behavior::Hits(1)),
            (RetrievalStrategy::Bm25, Behavior::Hits(1)),
        ])
        .with_query_transformer(Arc::new(TwoQueries));
        let c = o.retrieve_candidates(&Query::new("first")).await.unwrap();
        assert_eq!(c.queries, vec!["first".to_string(), "second".to_string()]);
        let idx: Vec<_> = c.lists.iter().map(|l| l.query_index).collect();
        assert_eq!(idx, vec![0, 0, 1, 1]);
        assert!(c.failed.is_empty());
    }

    #[tokio::test]
    async fn retrieve_candidates_reports_failures_once() {
        let mut o = orch(vec![
            (RetrievalStrategy::Vector, Behavior::Fail),
            (RetrievalStrategy::Bm25, Behavior::Hang),
            (RetrievalStrategy::Graph, Behavior::Hits(1)),
        ])
        .with_query_transformer(Arc::new(TwoQueries));
        o.strategy_timeout = Duration::from_millis(50);
        let c = o.retrieve_candidates(&Query::new("first")).await.unwrap();
        assert_eq!(c.failed.len(), 2);
        assert_eq!(c.lists.len(), 2);
        let bm25 = c
            .failed
            .iter()
            .find(|(s, _)| *s == RetrievalStrategy::Bm25)
            .unwrap();
        assert_eq!(bm25.1, "timeout");
        let vec_reason = c
            .failed
            .iter()
            .find(|(s, _)| *s == RetrievalStrategy::Vector)
            .unwrap();
        assert!(vec_reason.1.contains("boom"));
    }

    #[tokio::test]
    async fn retrieve_candidates_keeps_successful_empty_lists() {
        let o = orch(vec![(RetrievalStrategy::Vector, Behavior::Hits(0))]);
        let c = o.retrieve_candidates(&Query::new("q")).await.unwrap();
        assert_eq!(c.lists.len(), 1);
        assert!(c.lists[0].chunks.is_empty());
        assert!(c.failed.is_empty());
    }
}
