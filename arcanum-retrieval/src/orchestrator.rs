use arcanum_core::{traits::*, types::*, Result};
use crate::fusion::RrfFusion;
use std::{sync::Arc, time::Duration};
use tokio::time::timeout;

pub enum OrchestratorMode {
    Static(Vec<RetrievalStrategy>),
    ParallelFusion,
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

    fn active_retrievers(&self, _query: &Query) -> Vec<Arc<dyn Retriever>> {
        match &self.mode {
            OrchestratorMode::ParallelFusion => self.retrievers.clone(),
            OrchestratorMode::Static(strategies) => self.retrievers.iter()
                .filter(|r| strategies.contains(&r.strategy()))
                .cloned()
                .collect(),
        }
    }
}
