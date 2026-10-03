//! End-to-end Context assembly over the full ingestion harness.

#[path = "support/provenance_harness.rs"]
mod provenance_harness;

use arcanum_core::config::ContextConfig;
use arcanum_core::traits::{ApproxCl100kCounter, ChunkMetadataStore, TokenCounter};
use arcanum_core::types::{ContextRequest, RenderFormat};
use arcanum_engine::audit::AuditLogger;
use arcanum_engine::auth::{ApiKeyClaims, AuthMiddleware};
use arcanum_engine::services::context::ContextService;
use arcanum_graph::{GraphQueryPlanner, HopDecayScorer};
use arcanum_middleware::CircuitBreaker;
use arcanum_retrieval::{
    Bm25Retriever, GraphRetriever, OrchestratorMode, RetrievalOrchestrator, VectorRetriever,
};
use provenance_harness::*;
use std::sync::Arc;
use std::time::Duration;

#[tokio::test]
async fn context_over_full_ingestion_is_grounded_and_budgeted() {
    let fx = ingest_fixture().await;
    let orchestrator = Arc::new(
        RetrievalOrchestrator::new(OrchestratorMode::ParallelFusion)
            .add_retriever(Arc::new(VectorRetriever::new(
                fx.vector_store.clone(),
                Arc::new(KeywordEmbedder),
            )))
            .add_retriever(Arc::new(Bm25Retriever::new_global(
                fx.bm25.clone(),
                fx.chunk_metadata.clone(),
            )))
            .add_retriever(Arc::new(GraphRetriever::new(
                Arc::new(GraphQueryPlanner::new(Arc::new(ScriptedEnricher), 2)),
                fx.graph_store.clone(),
                Arc::new(HopDecayScorer::new(2)),
                fx.chunk_metadata.clone(),
            ))),
    );
    let svc = ContextService::new(
        orchestrator,
        fx.chunk_metadata.clone(),
        None,
        Arc::new(ApproxCl100kCounter),
        ContextConfig::default(),
        Arc::new(AuthMiddleware::new("a-32-char-secret-for-testing-ok!")),
        Arc::new(AuditLogger::new()),
        Arc::new(CircuitBreaker::new(
            "vector_store",
            5,
            Duration::from_secs(30),
        )),
    );
    let claims = ApiKeyClaims {
        user_id: "u1".into(),
        allowed_collections: vec![COLLECTION.into()],
        is_admin: false,
        exp: usize::MAX,
    };
    let resp = svc
        .assemble(
            ContextRequest {
                collection_id: COLLECTION.into(),
                query: Some("What does Acme Corp build?".into()),
                messages: None,
                token_budget: Some(600),
                background_share: None,
                candidate_k: None,
                render: Some(RenderFormat::Xml),
            },
            &claims,
        )
        .await
        .unwrap();

    assert!(!resp.passages.is_empty());
    let best = resp
        .passages
        .iter()
        .max_by(|a, b| a.score.partial_cmp(&b.score).unwrap())
        .unwrap();
    assert_eq!(best.document_id, fx.doc_a_id);
    for p in &resp.passages {
        let doc = if p.document_id == fx.doc_a_id {
            &fx.doc_a_text
        } else {
            &fx.doc_b_text
        };
        assert_eq!(&doc[p.offset_start..p.offset_end], p.text);
        for id in &p.chunk_ids {
            assert!(fx.chunk_metadata.get(id).await.unwrap().is_some());
        }
    }
    assert!(resp.passages.iter().any(|p| p.strategies.len() >= 2));
    let rendered = resp.rendered.as_deref().unwrap();
    assert!(ApproxCl100kCounter::new().count(rendered) <= 600);
    assert_eq!(resp.usage.counter, "approx_cl100k");
}
