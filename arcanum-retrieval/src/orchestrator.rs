use arcanum_core::{traits::*, types::*, Result};
use crate::fusion::RrfFusion;
use std::{sync::Arc, time::Duration};
use tokio::time::timeout;

pub enum OrchestratorMode {
    Static(Vec<RetrievalStrategy>),
    ParallelFusion,
    QueryClassified,
}

pub struct RetrievalOrchestrator {
    mode: OrchestratorMode,
    retrievers: Vec<Arc<dyn Retriever>>,
    strategy_timeout: Duration,
}

impl RetrievalOrchestrator {
    pub fn new(mode: OrchestratorMode) -> Self {
        Self { mode, retrievers: vec![], strategy_timeout: Duration::from_secs(5) }
    }

    pub fn add_retriever(mut self, r: Arc<dyn Retriever>) -> Self {
        self.retrievers.push(r);
        self
    }

    pub async fn retrieve(&self, query: &Query) -> Result<RetrievalResult> {
        let active = self.active_retrievers(query);
        let tasks: Vec<_> = active.iter().map(|r| {
            let r = r.clone();
            let q = query.clone();
            let t = self.strategy_timeout;
            tokio::spawn(async move {
                let result = timeout(t, r.retrieve(&q)).await;
                let strategy = r.strategy();
                match result {
                    Ok(Ok(chunks)) => Some((strategy, chunks)),
                    _ => None,
                }
            })
        }).collect();

        let mut strategy_results = vec![];
        for task in tasks {
            if let Ok(Some(r)) = task.await { strategy_results.push(r); }
        }

        let fused = RrfFusion::fuse(strategy_results, 60.0);
        let strategy_scores: std::collections::HashMap<String, f32> = fused.iter()
            .map(|c| (format!("{:?}", c.strategy), c.score)).collect();

        Ok(RetrievalResult {
            chunks: fused,
            citations: vec![],
            strategy_scores,
            confidence: 0.8,
        })
    }

    fn active_retrievers(&self, query: &Query) -> Vec<Arc<dyn Retriever>> {
        match &self.mode {
            OrchestratorMode::ParallelFusion => self.retrievers.clone(),
            OrchestratorMode::Static(strategies) => self.retrievers.iter()
                .filter(|r| strategies.contains(&r.strategy()))
                .cloned()
                .collect(),
            OrchestratorMode::QueryClassified => {
                let selected = classify_query(&query.text);
                self.retrievers.iter()
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
    let raptor_signals = ["summarize", "summarise", "overview", "across all", "throughout"];
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
    query.split_whitespace()
        .filter(|w| w.chars().next().map(|c| c.is_uppercase()).unwrap_or(false))
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_entity_query_includes_graph() {
        let strategies = classify_query("who is the CEO of Anthropic?");
        assert!(strategies.contains(&RetrievalStrategy::Graph), "Entity query should include Graph");
    }

    #[test]
    fn test_classify_summary_query_includes_raptor() {
        let strategies = classify_query("summarize the entire document");
        assert!(strategies.contains(&RetrievalStrategy::Raptor), "Summary query should include Raptor");
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
}
