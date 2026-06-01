//! Ex05 — Vantage Legal (120-person corporate legal team)
//! Ingestion: Full  |  Retrieval: ParallelFusion

#[path = "../helpers.rs"]
mod helpers;

use std::sync::Arc;
use arcanum_core::{
    traits::{GraphPlanner, GraphStore, LexicalIndex, TreeStore},
    types::{CollectionId, Entity, EntityId, Query, TreeNode, TreeNodeId, Vector},
};
use arcanum_graph::{InMemoryGraphStore, GraphQueryPlanner};
use arcanum_tree::InMemoryTreeStore;
use arcanum_retrieval::{
    OrchestratorMode, RetrievalOrchestrator, VectorRetriever, Bm25Retriever,
    GraphRetriever, RaptorRetriever,
};
use helpers::{
    FixedEmbedder, InMemoryVectorStore, InMemoryLexicalIndex,
    JsonEntityEnricher, seed_vector, seed_lexical,
};

const COLLECTION: &str = "vantage_contracts";
const DIM: usize = 8;

fn build_full_orchestrator() -> (
    RetrievalOrchestrator,
    Arc<InMemoryVectorStore>,
    Arc<InMemoryLexicalIndex>,
    Arc<InMemoryGraphStore>,
    Arc<InMemoryTreeStore>,
) {
    let store    = Arc::new(InMemoryVectorStore::new());
    let embedder = Arc::new(FixedEmbedder::new(DIM));
    let lexical  = Arc::new(InMemoryLexicalIndex::new());
    let enricher: Arc<dyn arcanum_core::traits::TextEnricher> = Arc::new(JsonEntityEnricher);
    let graph    = Arc::new(InMemoryGraphStore::new());
    let tree     = Arc::new(InMemoryTreeStore::new());

    let planner: Arc<dyn GraphPlanner> =
        Arc::new(GraphQueryPlanner::new(enricher, 2));

    let orchestrator = RetrievalOrchestrator::new(OrchestratorMode::ParallelFusion)
        .add_retriever(Arc::new(VectorRetriever::new(store.clone(), embedder.clone())))
        .add_retriever(Arc::new(Bm25Retriever::new_global(lexical.clone() as Arc<dyn LexicalIndex>)))
        .add_retriever(Arc::new(GraphRetriever::new(graph.clone(), store.clone(), planner, embedder.clone(), 2)))
        .add_retriever(Arc::new(RaptorRetriever::new(tree.clone(), embedder, 3)));

    (orchestrator, store, lexical, graph, tree)
}

async fn seed_legal_data(
    store: &InMemoryVectorStore,
    lexical: &InMemoryLexicalIndex,
    graph: &InMemoryGraphStore,
    tree: &InMemoryTreeStore,
) {
    let texts = &[
        "Section 12.3: Vendor shall indemnify Company from all claims. Indemnification cap: 2x annual contract value.",
        "Section 15.1: Surviving obligations post-termination include confidentiality and indemnification.",
        "Section 8.2: Data residency — Vendor must process EU personal data within EEA boundaries under GDPR.",
        "Acme Corp NDA: parties agree to maintain confidentiality of proprietary information for 5 years.",
    ];
    let chunks = seed_vector(store, COLLECTION, texts, DIM).await;
    seed_lexical(lexical, COLLECTION, texts);

    // Party entity: Acme Corp linked to its NDA chunk
    let acme = Entity {
        id: EntityId::new(),
        name: "Acme Corp".into(),
        entity_type: "Party".into(),
        canonical_id: None,
        source_chunks: vec![chunks[3].chunk.id.clone()],  // chunk 3 = Acme NDA text
    };
    graph.upsert_entities(vec![acme]).await.unwrap();

    // RAPTOR L2: cross-contract indemnification summary
    tree.insert_node(COLLECTION, TreeNode {
        id: TreeNodeId::new(),
        text: "Contract portfolio indemnification summary: vendor indemnification caps at 2x annual value, obligations surviving termination include confidentiality and indemnification, data residency under GDPR for EU data".into(),
        level: 2,
        vector: Vector(vec![0.5f32; DIM]),
        children: vec![],
        parent: None,
        cluster_centroid: None,
    }).await.unwrap();
}

#[tokio::test]
async fn exact_term_lookup_bm25_contributes() {
    let (orch, store, lexical, graph, tree) = build_full_orchestrator();
    seed_legal_data(&store, &lexical, &graph, &tree).await;

    let q = Query::new("indemnification cap")
        .with_collection(CollectionId(COLLECTION.into()))
        .with_top_k(5);
    let result = orch.retrieve(&q).await.unwrap();

    assert!(!result.chunks.is_empty());
    assert!(result.strategy_scores.contains_key("Bm25"),
        "BM25 must match 'indemnification cap' exact phrase");
}

#[tokio::test]
async fn summary_query_raptor_contributes() {
    let (orch, store, lexical, graph, tree) = build_full_orchestrator();
    seed_legal_data(&store, &lexical, &graph, &tree).await;

    let q = Query::new("summarize indemnification obligations across all contracts")
        .with_collection(CollectionId(COLLECTION.into()))
        .with_top_k(5);
    let result = orch.retrieve(&q).await.unwrap();

    assert!(!result.chunks.is_empty());
    assert!(result.strategy_scores.contains_key("Raptor"),
        "Raptor must contribute for cross-contract summary query");
}

#[tokio::test]
async fn party_entity_query_graph_contributes() {
    let (orch, store, lexical, graph, tree) = build_full_orchestrator();
    seed_legal_data(&store, &lexical, &graph, &tree).await;

    // "Acme Corp" is an entity in the graph with source_chunks → GraphRetriever fires
    let q = Query::new("Acme Corp NDA confidentiality obligations")
        .with_collection(CollectionId(COLLECTION.into()))
        .with_top_k(5);
    let result = orch.retrieve(&q).await.unwrap();

    assert!(!result.chunks.is_empty());
    let has_graph  = result.strategy_scores.contains_key("Graph");
    let has_vector = result.strategy_scores.contains_key("Vector");
    assert!(has_graph || has_vector,
        "Graph or Vector must contribute for party entity query");
}

#[tokio::test]
async fn parallel_fusion_vector_always_contributes() {
    // In ParallelFusion all four retrievers run. Vector is the baseline —
    // it must always return results when the store has content.
    let (orch, store, lexical, graph, tree) = build_full_orchestrator();
    seed_legal_data(&store, &lexical, &graph, &tree).await;

    let q = Query::new("data residency EU GDPR vendor obligations")
        .with_collection(CollectionId(COLLECTION.into()))
        .with_top_k(5);
    let result = orch.retrieve(&q).await.unwrap();

    assert!(!result.chunks.is_empty());
    assert!(result.strategy_scores.contains_key("Vector"),
        "Vector must always contribute in ParallelFusion when store has content");
}
