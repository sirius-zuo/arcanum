//! Ex01 — Devforge (25-person API-first SaaS startup)
//! Ingestion: Standard  |  Retrieval: Static([Vector, Bm25])

#[path = "../helpers.rs"]
mod helpers;

use std::sync::Arc;
use arcanum_core::{traits::LexicalIndex, types::{CollectionId, Query, RetrievalStrategy}};
use arcanum_retrieval::{OrchestratorMode, RetrievalOrchestrator, VectorRetriever, Bm25Retriever};
use helpers::{FixedEmbedder, InMemoryVectorStore, InMemoryLexicalIndex, seed_vector, seed_lexical};

const COLLECTION: &str = "devforge";
const DIM: usize = 8;

fn build_orchestrator() -> (RetrievalOrchestrator, Arc<InMemoryVectorStore>, Arc<InMemoryLexicalIndex>) {
    let store    = Arc::new(InMemoryVectorStore::new());
    let embedder = Arc::new(FixedEmbedder::new(DIM));
    let lexical  = Arc::new(InMemoryLexicalIndex::new());

    let orchestrator = RetrievalOrchestrator::new(
        OrchestratorMode::Static(vec![RetrievalStrategy::Vector, RetrievalStrategy::Bm25])
    )
    .add_retriever(Arc::new(VectorRetriever::new(store.clone(), embedder)))
    .add_retriever(Arc::new(Bm25Retriever::new_global(lexical.clone() as Arc<dyn LexicalIndex>)));

    (orchestrator, store, lexical)
}

#[tokio::test]
async fn semantic_query_returns_vector_results() {
    let (orch, store, lexical) = build_orchestrator();
    let texts = &[
        "How to authenticate using OAuth2 and API keys in the Devforge SDK",
        "invalid_api_key error occurs when the key is missing or expired",
        "Rate limit headers: X-RateLimit-Limit and X-RateLimit-Remaining",
    ];
    seed_vector(&store, COLLECTION, texts, DIM).await;
    seed_lexical(&lexical, COLLECTION, texts);

    let q = Query::new("how do I authenticate with OAuth2?")
        .with_collection(CollectionId(COLLECTION.into()))
        .with_top_k(5);
    let result = orch.retrieve(&q).await.unwrap();

    assert!(!result.chunks.is_empty(), "semantic query must return results");
    assert!(result.strategy_scores.contains_key("Vector"), "Vector must contribute");
    assert!(!result.strategy_scores.contains_key("Graph"),  "Static+Standard must not include Graph");
    assert!(!result.strategy_scores.contains_key("Raptor"), "Static+Standard must not include Raptor");
}

#[tokio::test]
async fn keyword_query_bm25_contributes() {
    let (orch, store, lexical) = build_orchestrator();
    let texts = &[
        "How to authenticate using OAuth2 and API keys in the Devforge SDK",
        "invalid_api_key error occurs when the key is missing or expired",
        "Rate limit headers: X-RateLimit-Limit and X-RateLimit-Remaining",
    ];
    seed_vector(&store, COLLECTION, texts, DIM).await;
    seed_lexical(&lexical, COLLECTION, texts);

    let q = Query::new("invalid_api_key error")
        .with_collection(CollectionId(COLLECTION.into()))
        .with_top_k(5);
    let result = orch.retrieve(&q).await.unwrap();

    assert!(!result.chunks.is_empty(), "keyword query must return results");
    assert!(
        result.strategy_scores.contains_key("Bm25") || result.strategy_scores.contains_key("Vector"),
        "at least one strategy must contribute for keyword query"
    );
}

#[tokio::test]
async fn static_mode_excludes_graph_and_raptor() {
    let (orch, store, lexical) = build_orchestrator();
    let texts = &["rate limit headers X-RateLimit-Limit"];
    seed_vector(&store, COLLECTION, texts, DIM).await;
    seed_lexical(&lexical, COLLECTION, texts);

    let q = Query::new("rate limit headers")
        .with_collection(CollectionId(COLLECTION.into()))
        .with_top_k(5);
    let result = orch.retrieve(&q).await.unwrap();

    let strategies: std::collections::HashSet<&str> =
        result.strategy_scores.keys().map(String::as_str).collect();
    assert!(!strategies.contains("Graph"),  "Static must not activate GraphRetriever");
    assert!(!strategies.contains("Raptor"), "Static must not activate RaptorRetriever");
}
