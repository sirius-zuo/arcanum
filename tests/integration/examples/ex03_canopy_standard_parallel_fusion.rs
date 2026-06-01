//! Ex03 — Canopy Co. (80-person D2C outdoor gear brand)
//! Ingestion: Standard  |  Retrieval: ParallelFusion (RRF)

#[path = "../helpers.rs"]
mod helpers;

use std::sync::Arc;
use arcanum_core::{traits::LexicalIndex, types::{CollectionId, Query}};
use arcanum_retrieval::{OrchestratorMode, RetrievalOrchestrator, VectorRetriever, Bm25Retriever};
use helpers::{FixedEmbedder, InMemoryVectorStore, InMemoryLexicalIndex, seed_vector, seed_lexical};

const COLLECTION: &str = "canopy";
const DIM: usize = 8;

fn build_orchestrator() -> (RetrievalOrchestrator, Arc<InMemoryVectorStore>, Arc<InMemoryLexicalIndex>) {
    let store    = Arc::new(InMemoryVectorStore::new());
    let embedder = Arc::new(FixedEmbedder::new(DIM));
    let lexical  = Arc::new(InMemoryLexicalIndex::new());

    let orchestrator = RetrievalOrchestrator::new(OrchestratorMode::ParallelFusion)
        .add_retriever(Arc::new(VectorRetriever::new(store.clone(), embedder)))
        .add_retriever(Arc::new(Bm25Retriever::new_global(lexical.clone() as Arc<dyn LexicalIndex>)));

    (orchestrator, store, lexical)
}

#[tokio::test]
async fn semantic_query_vector_contributes() {
    let (orch, store, lexical) = build_orchestrator();
    let texts = &[
        "Cascade 3000 jacket: GORE-TEX membrane, 20000mm waterproof rating, 3-layer construction",
        "Summit sleeping bag: rated to -20C, 800-fill goose down, compression sack included",
        "Trailblazer tent: 3-season, 2-person, freestanding, 2.3kg, UV-resistant ripstop nylon",
    ];
    seed_vector(&store, COLLECTION, texts, DIM).await;
    seed_lexical(&lexical, COLLECTION, texts);

    let q = Query::new("best jacket for winter hiking")
        .with_collection(CollectionId(COLLECTION.into()))
        .with_top_k(5);
    let result = orch.retrieve(&q).await.unwrap();

    assert!(!result.chunks.is_empty(), "semantic query must return results");
    assert!(result.strategy_scores.contains_key("Vector"),
        "Vector must run in ParallelFusion and return results");
}

#[tokio::test]
async fn keyword_query_bm25_contributes() {
    let (orch, store, lexical) = build_orchestrator();
    let texts = &[
        "SKU TN-4892: Trailblazer tent, weight 2300g, packed size 55x18cm",
        "SKU JK-2201: Cascade 3000 jacket, sizes S-XXL, GORE-TEX waterproof",
        "SKU SB-9901: Summit sleeping bag, -20C rating, 1800g",
    ];
    seed_vector(&store, COLLECTION, texts, DIM).await;
    seed_lexical(&lexical, COLLECTION, texts);

    let q = Query::new("SKU TN-4892 weight")
        .with_collection(CollectionId(COLLECTION.into()))
        .with_top_k(5);
    let result = orch.retrieve(&q).await.unwrap();

    assert!(!result.chunks.is_empty());
    assert!(result.strategy_scores.contains_key("Bm25"),
        "Bm25 must match exact SKU keyword in ParallelFusion");
}

#[tokio::test]
async fn parallel_fusion_no_graph_or_raptor_in_standard_setup() {
    // ParallelFusion runs ALL wired retrievers. No GraphRetriever or RaptorRetriever
    // are wired → their strategies must never appear in results.
    let (orch, store, lexical) = build_orchestrator();
    let texts = &["waterproof jacket outdoor rain protection"];
    seed_vector(&store, COLLECTION, texts, DIM).await;
    seed_lexical(&lexical, COLLECTION, texts);

    let q = Query::new("waterproof jacket")
        .with_collection(CollectionId(COLLECTION.into()))
        .with_top_k(5);
    let result = orch.retrieve(&q).await.unwrap();

    assert!(!result.strategy_scores.contains_key("Graph"),
        "No GraphRetriever wired in Standard → Graph must not appear");
    assert!(!result.strategy_scores.contains_key("Raptor"),
        "No RaptorRetriever wired in Standard → Raptor must not appear");
}

#[tokio::test]
async fn parallel_fusion_both_strategies_run_concurrently() {
    // Plant the same keyword in both stores so both retrievers fire.
    // RRF should surface the overlapping chunk at the top.
    let (orch, store, lexical) = build_orchestrator();
    let texts = &["waterproof GORE-TEX breathable outdoor jacket"];
    seed_vector(&store, COLLECTION, texts, DIM).await;
    seed_lexical(&lexical, COLLECTION, texts);

    let q = Query::new("waterproof jacket")
        .with_collection(CollectionId(COLLECTION.into()))
        .with_top_k(5);
    let result = orch.retrieve(&q).await.unwrap();

    assert!(!result.chunks.is_empty());
    let has_vector = result.strategy_scores.contains_key("Vector");
    let has_bm25   = result.strategy_scores.contains_key("Bm25");
    assert!(has_vector && has_bm25,
        "ParallelFusion must run both Vector and Bm25 when both stores have matching content");
}
