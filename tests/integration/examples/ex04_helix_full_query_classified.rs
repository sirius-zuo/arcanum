//! Ex04 — Helix Labs (200-person drug discovery startup)
//! Ingestion: Full (entity graph + RAPTOR tree + contextual enrichment)
//! Retrieval: QueryClassified

#[path = "../helpers.rs"]
mod helpers;

use std::sync::Arc;
use arcanum_core::{
    traits::{GraphPlanner, GraphStore, LexicalIndex, TreeStore},
    types::{
        CollectionId, Entity, EntityId, Query,
        RetrievalStrategy, TreeNode, TreeNodeId, Vector,
    },
};
use arcanum_graph::{InMemoryGraphStore, GraphQueryPlanner};
use arcanum_tree::InMemoryTreeStore;
use arcanum_retrieval::{
    OrchestratorMode, RetrievalOrchestrator, VectorRetriever, Bm25Retriever,
    GraphRetriever, RaptorRetriever, classify_query,
};
use helpers::{
    FixedEmbedder, InMemoryVectorStore, InMemoryLexicalIndex,
    JsonEntityEnricher, seed_vector, seed_lexical,
};

const COLLECTION: &str = "helix_research";
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

    let orchestrator = RetrievalOrchestrator::new(OrchestratorMode::QueryClassified)
        .add_retriever(Arc::new(VectorRetriever::new(store.clone(), embedder.clone())))
        .add_retriever(Arc::new(Bm25Retriever::new_global(lexical.clone() as Arc<dyn LexicalIndex>)))
        .add_retriever(Arc::new(GraphRetriever::new(graph.clone(), store.clone(), planner, embedder.clone(), 2)))
        .add_retriever(Arc::new(RaptorRetriever::new(tree.clone(), embedder, 3)));

    (orchestrator, store, lexical, graph, tree)
}

async fn seed_helix_data(
    store: &InMemoryVectorStore,
    lexical: &InMemoryLexicalIndex,
    graph: &InMemoryGraphStore,
    tree: &InMemoryTreeStore,
) {
    let texts = &[
        "Compound 17g inhibits EGFR with IC50 of 2.3nM in kinase assay results",
        "EGFR overexpression correlates with poor prognosis in NSCLC patients",
        "Phase 2 trial: dose-limiting toxicity observed at 400mg in cohort 3",
        "JAK inhibitor program: adverse event profile shows elevated liver enzymes",
        "CRISPR Cas9 delivery via lipid nanoparticles in neuronal cell lines",
    ];
    let chunks = seed_vector(store, COLLECTION, texts, DIM).await;
    seed_lexical(lexical, COLLECTION, texts);

    // Seed graph: EGFR protein entity linked to its source chunk
    let egfr = Entity {
        id: EntityId::new(),
        name: "EGFR".into(),
        entity_type: "Protein".into(),
        canonical_id: None,
        source_chunks: vec![chunks[1].chunk.id.clone()],  // chunk 1 = EGFR overexpression text
    };
    graph.upsert_entities(vec![egfr]).await.unwrap();

    // Seed RAPTOR L2 root: whole-corpus summary
    tree.insert_node(COLLECTION, TreeNode {
        id: TreeNodeId::new(),
        text: "Helix research corpus: EGFR-targeted compounds, JAK inhibitor toxicity profiles, CRISPR delivery mechanisms in neuronal tissue".into(),
        level: 2,
        vector: Vector(vec![0.5f32; DIM]),
        children: vec![],
        parent: None,
        cluster_centroid: None,
    }).await.unwrap();

    // Seed RAPTOR L1: trial summary
    tree.insert_node(COLLECTION, TreeNode {
        id: TreeNodeId::new(),
        text: "Phase 2 trial summary: dose-limiting toxicity at 400mg, MTD established at 300mg, adverse events including elevated liver enzymes across JAK inhibitor cohorts".into(),
        level: 1,
        vector: Vector(vec![0.4f32; DIM]),
        children: vec![],
        parent: None,
        cluster_centroid: None,
    }).await.unwrap();
}

#[tokio::test]
async fn classify_entity_query_routes_to_graph() {
    let strategies = classify_query("does Compound EGFR bind to receptor kinase?");
    assert!(strategies.contains(&RetrievalStrategy::Graph),
        "proper-noun entity query should route to Graph");
}

#[tokio::test]
async fn classify_summary_query_routes_to_raptor() {
    let strategies = classify_query("summarize adverse events across all Phase 2 trials");
    assert!(strategies.contains(&RetrievalStrategy::Raptor),
        "'across all' summary signal should route to Raptor");
}

#[tokio::test]
async fn classify_semantic_query_routes_to_vector_bm25() {
    let strategies = classify_query("lipid nanoparticle delivery mechanisms in tissue");
    assert!(strategies.contains(&RetrievalStrategy::Vector));
    assert!(strategies.contains(&RetrievalStrategy::Bm25));
    assert!(!strategies.contains(&RetrievalStrategy::Raptor));
}

#[tokio::test]
async fn summary_query_raptor_contributes() {
    let (orch, store, lexical, graph, tree) = build_full_orchestrator();
    seed_helix_data(&store, &lexical, &graph, &tree).await;

    let q = Query::new("summarize adverse events across all Phase 2 JAK inhibitor trials")
        .with_collection(CollectionId(COLLECTION.into()))
        .with_top_k(5);
    let result = orch.retrieve(&q).await.unwrap();

    assert!(!result.chunks.is_empty(), "summary query must return results from RAPTOR tree");
    assert!(result.strategy_scores.contains_key("Raptor"),
        "Raptor must contribute for 'summarize across all' query");
}

#[tokio::test]
async fn entity_query_graph_or_vector_contributes() {
    let (orch, store, lexical, graph, tree) = build_full_orchestrator();
    seed_helix_data(&store, &lexical, &graph, &tree).await;

    // JsonEntityEnricher extracts "EGFR" → GraphRetriever finds the entity →
    // entity has source_chunks → vector search fires → results with strategy=Graph
    let q = Query::new("EGFR inhibition compound assay")
        .with_collection(CollectionId(COLLECTION.into()))
        .with_top_k(5);
    let result = orch.retrieve(&q).await.unwrap();

    assert!(!result.chunks.is_empty(), "entity query must return results");
    let has_graph  = result.strategy_scores.contains_key("Graph");
    let has_vector = result.strategy_scores.contains_key("Vector");
    assert!(has_graph || has_vector,
        "Graph or Vector must contribute for entity query");
}

#[tokio::test]
async fn semantic_query_no_graph_or_raptor() {
    let (orch, store, lexical, graph, tree) = build_full_orchestrator();
    seed_helix_data(&store, &lexical, &graph, &tree).await;

    // Broad semantic query — no entity or summarize signals → [Vector, Bm25]
    let q = Query::new("lipid nanoparticle delivery in neuronal tissue")
        .with_collection(CollectionId(COLLECTION.into()))
        .with_top_k(5);
    let result = orch.retrieve(&q).await.unwrap();

    assert!(!result.chunks.is_empty());
    assert!(!result.strategy_scores.contains_key("Graph"),
        "broad semantic query must not activate GraphRetriever");
    assert!(!result.strategy_scores.contains_key("Raptor"),
        "broad semantic query must not activate RaptorRetriever");
}
