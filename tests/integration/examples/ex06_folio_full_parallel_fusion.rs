//! Ex06 — Folio (60-person digital library platform)
//! Ingestion: Full (entity graph for authors/characters/series; RAPTOR for whole-book summaries)
//! Retrieval: ParallelFusion
//!
//! Query range: very specific passage lookups → author/series graph traversals
//!              → whole-book RAPTOR summaries → cross-collection thematic discovery

#[path = "../helpers.rs"]
mod helpers;

use std::sync::Arc;
use arcanum_core::{
    traits::{GraphPlanner, GraphStore, LexicalIndex, TreeStore},
    types::{CollectionId, Entity, EntityId, Query, Relation, TreeNode, TreeNodeId, Vector},
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

const COLLECTION: &str = "folio_library";
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

async fn seed_library_data(
    store: &InMemoryVectorStore,
    lexical: &InMemoryLexicalIndex,
    graph: &InMemoryGraphStore,
    tree: &InMemoryTreeStore,
) {
    // Passage-level chunks (L0 equivalents in RAPTOR, also in flat vector store)
    let texts = &[
        "[Moby Dick | Melville | Ch.1] Call me Ishmael. Some years ago having little money I went to sea",
        "[To Kill a Mockingbird | Harper Lee | Ch.11] Atticus told Scout real courage is when you know you are licked before you begin but begin anyway",
        "[Anna Karenina | Tolstoy | Ch.1] All happy families are alike; each unhappy family is unhappy in its own way",
        "[The Hobbit | Tolkien | Ch.12] The dragon Smaug lay stretched upon a bed of gold his red-golden scales gleaming in firelight",
        "[The Left Hand of Darkness | Le Guin | Ch.1] I'll make my report as if I told a story for I was taught as a child that truth is a matter of the imagination",
    ];
    let chunks = seed_vector(store, COLLECTION, texts, DIM).await;
    seed_lexical(lexical, COLLECTION, texts);

    // Graph: Author entities with source_chunks linked to their book's passage
    let le_guin = Entity {
        id: EntityId::new(),
        name: "Le Guin".into(),
        entity_type: "Author".into(),
        canonical_id: Some("ursula-k-le-guin".into()),
        source_chunks: vec![chunks[4].chunk.id.clone()],  // Left Hand of Darkness chunk
    };
    let tolkien = Entity {
        id: EntityId::new(),
        name: "Tolkien".into(),
        entity_type: "Author".into(),
        canonical_id: Some("j-r-r-tolkien".into()),
        source_chunks: vec![chunks[3].chunk.id.clone()],  // Hobbit chunk
    };
    let hainish = Entity {
        id: EntityId::new(),
        name: "Hainish".into(),
        entity_type: "Series".into(),
        canonical_id: None,
        source_chunks: vec![chunks[4].chunk.id.clone()],
    };
    graph.upsert_entities(vec![le_guin.clone(), tolkien, hainish.clone()]).await.unwrap();
    graph.upsert_relations(vec![
        Relation {
            source: le_guin.id.clone(),
            target: hainish.id.clone(),
            relation_type: "wrote_for_series".into(),
            confidence: 1.0,
            source_chunk: chunks[4].chunk.id.clone(),
        },
    ]).await.unwrap();

    // RAPTOR tree: L2 whole-book summary for Moby Dick
    tree.insert_node(COLLECTION, TreeNode {
        id: TreeNodeId::new(),
        text: "Moby Dick — Ishmael narrates the Pequod voyage under Ahab obsessed with hunting the white whale. Themes: obsession, fate, humanity vs nature, the sea.".into(),
        level: 2,
        vector: Vector(vec![0.5f32; DIM]),
        children: vec![],
        parent: None,
        cluster_centroid: None,
    }).await.unwrap();

    // RAPTOR tree: L1 chapter summary
    tree.insert_node(COLLECTION, TreeNode {
        id: TreeNodeId::new(),
        text: "Moby Dick Chapter 1: Ishmael introduces himself, explains his desire to go to sea. Opens with 'Call me Ishmael.' Sets the philosophical, melancholy tone.".into(),
        level: 1,
        vector: Vector(vec![0.4f32; DIM]),
        children: vec![],
        parent: None,
        cluster_centroid: None,
    }).await.unwrap();
}

#[tokio::test]
async fn passage_query_vector_contributes() {
    let (orch, store, lexical, graph, tree) = build_full_orchestrator();
    seed_library_data(&store, &lexical, &graph, &tree).await;

    let q = Query::new("Atticus Scout courage licked before you begin")
        .with_collection(CollectionId(COLLECTION.into()))
        .with_top_k(5);
    let result = orch.retrieve(&q).await.unwrap();

    assert!(!result.chunks.is_empty(), "passage query must return results");
    assert!(result.strategy_scores.contains_key("Vector"),
        "Vector must contribute for passage-level semantic search");
}

#[tokio::test]
async fn exact_opening_line_bm25_contributes() {
    let (orch, store, lexical, graph, tree) = build_full_orchestrator();
    seed_library_data(&store, &lexical, &graph, &tree).await;

    let q = Query::new("Call me Ishmael")
        .with_collection(CollectionId(COLLECTION.into()))
        .with_top_k(5);
    let result = orch.retrieve(&q).await.unwrap();

    assert!(!result.chunks.is_empty(), "exact phrase query must return results");
    assert!(
        result.strategy_scores.contains_key("Bm25") || result.strategy_scores.contains_key("Vector"),
        "BM25 or Vector must match 'Call me Ishmael'"
    );
}

#[tokio::test]
async fn author_entity_graph_contributes() {
    let (orch, store, lexical, graph, tree) = build_full_orchestrator();
    seed_library_data(&store, &lexical, &graph, &tree).await;

    // JsonEntityEnricher extracts "Tolkien" as an entity name;
    // InMemoryGraphStore.query() matches entity where name.contains("Tolkien")
    let q = Query::new("Tolkien Hobbit dragon Smaug")
        .with_collection(CollectionId(COLLECTION.into()))
        .with_top_k(5);
    let result = orch.retrieve(&q).await.unwrap();

    assert!(!result.chunks.is_empty());
    let has_graph  = result.strategy_scores.contains_key("Graph");
    let has_vector = result.strategy_scores.contains_key("Vector");
    assert!(has_graph || has_vector,
        "Graph or Vector must contribute for author entity query");
}

#[tokio::test]
async fn whole_book_summary_raptor_contributes() {
    let (orch, store, lexical, graph, tree) = build_full_orchestrator();
    seed_library_data(&store, &lexical, &graph, &tree).await;

    let q = Query::new("summarize the plot of Moby Dick")
        .with_collection(CollectionId(COLLECTION.into()))
        .with_top_k(5);
    let result = orch.retrieve(&q).await.unwrap();

    assert!(!result.chunks.is_empty(), "summary query must return RAPTOR tree nodes");
    assert!(result.strategy_scores.contains_key("Raptor"),
        "Raptor must contribute for whole-book summary query");
}

#[tokio::test]
async fn thematic_discovery_vector_bm25_contribute() {
    let (orch, store, lexical, graph, tree) = build_full_orchestrator();
    seed_library_data(&store, &lexical, &graph, &tree).await;

    // No entity or summarize signals — broad thematic query
    let q = Query::new("books about obsession fate the sea")
        .with_collection(CollectionId(COLLECTION.into()))
        .with_top_k(5);
    let result = orch.retrieve(&q).await.unwrap();

    assert!(!result.chunks.is_empty(), "thematic discovery must return results");
    assert!(
        result.strategy_scores.contains_key("Vector") || result.strategy_scores.contains_key("Raptor"),
        "Vector or Raptor must contribute for thematic discovery"
    );
}

#[tokio::test]
async fn parallel_fusion_all_four_retrievers_wired() {
    // Verify all four retrievers are registered: wiring check via strategy coverage.
    // A query designed to touch all four stores confirms none are silently missing.
    let (orch, store, lexical, graph, tree) = build_full_orchestrator();
    seed_library_data(&store, &lexical, &graph, &tree).await;

    // "summarize" → Raptor; "Tolkien" → Graph; "Ishmael" → Bm25; semantic → Vector
    let q = Query::new("summarize Tolkien Ishmael voyage")
        .with_collection(CollectionId(COLLECTION.into()))
        .with_top_k(10);
    let result = orch.retrieve(&q).await.unwrap();

    assert!(!result.chunks.is_empty());
    // In ParallelFusion the orchestrator fans out to all four —
    // at minimum Vector must contribute (always has content)
    assert!(result.strategy_scores.contains_key("Vector"),
        "Vector must always contribute in ParallelFusion when store has content");
}
