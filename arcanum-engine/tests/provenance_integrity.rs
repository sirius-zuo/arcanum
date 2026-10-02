//! End-to-end reproductions of provenance loss across retrieval backends.
//! Each test is ignored with the plan task that fixes it; that task un-ignores it.

#[path = "support/provenance_harness.rs"]
mod provenance_harness;

use arcanum_core::traits::{ChunkMetadataStore, EvidenceResolver, Retriever, TreeStore};
use arcanum_core::types::{CollectionId, Query};
use arcanum_evidence::resolver::DefaultEvidenceResolver;
use arcanum_graph::GraphQueryPlanner;
use arcanum_retrieval::{Bm25Retriever, GraphRetriever, RaptorRetriever};
use provenance_harness::*;
use std::sync::Arc;

fn query(text: &str, top_k: usize) -> Query {
    let mut q = Query::new(text);
    q.collection_id = Some(CollectionId(COLLECTION.into()));
    q.top_k = top_k;
    q
}

#[tokio::test]
#[ignore = "fixed by Task 14"]
async fn graph_retrieval_returns_source_chunks() {
    let fx = ingest_fixture().await;
    let retriever = GraphRetriever::new(
        fx.graph_store.clone(),
        fx.vector_store.clone(),
        Arc::new(GraphQueryPlanner::new(Arc::new(ScriptedEnricher), 2)),
        Arc::new(KeywordEmbedder),
        2,
    );
    let results = retriever
        .retrieve(&query("What does Acme Corp build?", 5))
        .await
        .unwrap();
    assert!(!results.is_empty(), "graph retrieval returned nothing");
    for r in &results {
        let c = &r.indexed_chunk.chunk;
        assert_eq!(c.document_id, fx.doc_a_id);
        assert_eq!(&fx.doc_a_text[c.position.start..c.position.end], c.text);
    }
}

#[tokio::test]
#[ignore = "fixed by Task 10"]
async fn bm25_results_carry_real_text_and_document_id() {
    let fx = ingest_fixture().await;
    let retriever = Bm25Retriever::new(CollectionId(COLLECTION.into()), fx.bm25.clone());
    let results = retriever.retrieve(&query("rockets", 5)).await.unwrap();
    assert!(!results.is_empty(), "bm25 returned nothing");
    for r in &results {
        let c = &r.indexed_chunk.chunk;
        assert_eq!(c.document_id, fx.doc_a_id);
        assert!(c.text.contains("rockets"), "text was {:?}", c.text);
        assert_eq!(&fx.doc_a_text[c.position.start..c.position.end], c.text);
    }
}

#[tokio::test]
#[ignore = "fixed by Task 11"]
async fn raptor_leaf_results_carry_provenance_and_offsets() {
    let fx = ingest_fixture().await;
    let retriever = RaptorRetriever::new(fx.tree_store.clone(), Arc::new(KeywordEmbedder), 3);
    let results = retriever.retrieve(&query("rockets", 20)).await.unwrap();
    // Level is currently encoded in position.index (Task 11 switches this to kind == Source).
    let leaves: Vec<_> = results
        .iter()
        .filter(|r| r.indexed_chunk.chunk.position.index == 0)
        .collect();
    assert!(!leaves.is_empty(), "no level-0 results");
    for r in leaves {
        let c = &r.indexed_chunk.chunk;
        assert!(
            fx.chunk_metadata.get(&c.id).await.unwrap().is_some(),
            "leaf chunk id {} is not in the registry",
            c.id.0
        );
        assert_eq!(c.provenance.document_version, 1);
        assert!(c.position.end > c.position.start);
    }
}

#[tokio::test]
#[ignore = "fixed by Task 8"]
async fn resolve_tree_node_reaches_tree_chunks() {
    let fx = ingest_fixture().await;
    let resolver = DefaultEvidenceResolver::new(
        fx.chunk_metadata.clone(),
        fx.version_store.clone(),
        Some(fx.tree_store.clone()),
        Some(fx.graph_store.clone()),
    );
    let level1 = fx.tree_store.get_level(COLLECTION, 1).await.unwrap();
    assert!(!level1.is_empty(), "fixture produced no level-1 tree nodes");
    for node in level1 {
        let chain = resolver.resolve_tree_node(&node.id).await.unwrap();
        assert_eq!(chain.root.children.len(), node.leaf_chunk_ids.len());
    }
}
