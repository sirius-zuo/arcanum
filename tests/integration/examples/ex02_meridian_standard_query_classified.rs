//! Ex02 — Meridian Consulting (150-person management consulting firm)
//! Ingestion: Standard  |  Retrieval: QueryClassified

#[path = "../helpers.rs"]
mod helpers;

use std::sync::Arc;
use arcanum_core::{traits::LexicalIndex, types::{CollectionId, Query, RetrievalStrategy}};
use arcanum_retrieval::{OrchestratorMode, RetrievalOrchestrator, VectorRetriever, Bm25Retriever, classify_query};
use helpers::{FixedEmbedder, InMemoryVectorStore, InMemoryLexicalIndex, seed_vector, seed_lexical};

const COLLECTION: &str = "meridian";
const DIM: usize = 8;

fn build_orchestrator() -> (RetrievalOrchestrator, Arc<InMemoryVectorStore>, Arc<InMemoryLexicalIndex>) {
    let store    = Arc::new(InMemoryVectorStore::new());
    let embedder = Arc::new(FixedEmbedder::new(DIM));
    let lexical  = Arc::new(InMemoryLexicalIndex::new());

    let orchestrator = RetrievalOrchestrator::new(OrchestratorMode::QueryClassified)
        .add_retriever(Arc::new(VectorRetriever::new(store.clone(), embedder)))
        .add_retriever(Arc::new(Bm25Retriever::new_global(lexical.clone() as Arc<dyn LexicalIndex>)));

    (orchestrator, store, lexical)
}

#[tokio::test]
async fn classify_keyword_lookup_routes_vector_bm25() {
    // "PTO accrual rate" — no summarize or entity signals → default [Vector, Bm25]
    let strategies = classify_query("PTO accrual rate for part-time employees");
    assert!(strategies.contains(&RetrievalStrategy::Vector));
    assert!(strategies.contains(&RetrievalStrategy::Bm25));
    assert!(!strategies.contains(&RetrievalStrategy::Raptor));
    assert!(!strategies.contains(&RetrievalStrategy::Graph));
}

#[tokio::test]
async fn classify_conceptual_query_routes_vector_bm25() {
    // Conceptual question, no entity/summarize signals → default [Vector, Bm25]
    let strategies = classify_query("how does maternity leave affect my performance review?");
    assert!(strategies.contains(&RetrievalStrategy::Vector));
    assert!(strategies.contains(&RetrievalStrategy::Bm25));
}

#[tokio::test]
async fn classify_summary_query_routes_raptor() {
    let strategies = classify_query("summarize all HR policies for new employees");
    assert!(strategies.contains(&RetrievalStrategy::Raptor),
        "summary signal must route to Raptor");
}

#[tokio::test]
async fn query_classified_policy_search_returns_results() {
    let (orch, store, lexical) = build_orchestrator();
    let texts = &[
        "PTO accrual: full-time earns 15 days, part-time earns 7.5 days per year",
        "Parental leave: 16 weeks paid leave for primary caregivers",
        "Performance review cycle runs annually in Q4 for permanent staff",
        "401k match: company matches 4% of salary up to IRS maximum",
    ];
    seed_vector(&store, COLLECTION, texts, DIM).await;
    seed_lexical(&lexical, COLLECTION, texts);

    let q = Query::new("PTO accrual rate for part-time employees")
        .with_collection(CollectionId(COLLECTION.into()))
        .with_top_k(5);
    let result = orch.retrieve(&q).await.unwrap();

    assert!(!result.chunks.is_empty(), "policy lookup must return results");
    assert!(!result.strategy_scores.contains_key("Graph"),  "Standard setup has no GraphRetriever");
    assert!(!result.strategy_scores.contains_key("Raptor"), "Standard setup has no RaptorRetriever");
}

#[tokio::test]
async fn query_classified_raptor_strategy_selected_but_no_retriever_wired() {
    // When the classifier emits Raptor but no RaptorRetriever is wired (Standard ingestion),
    // the orchestrator finds no matching retriever for that strategy.
    // Result: empty (all selected strategies return nothing).
    let (orch, _store, _lexical) = build_orchestrator();
    // No data seeded — isolates the "strategy selected but no retriever" case.

    let q = Query::new("summarize all HR policies")
        .with_collection(CollectionId(COLLECTION.into()))
        .with_top_k(5);
    let result = orch.retrieve(&q).await.unwrap();

    // Raptor strategy selected, but not wired → must not appear in scores
    assert!(!result.strategy_scores.contains_key("Raptor"),
        "RaptorRetriever is not wired in Standard setup; score must not appear");
}
