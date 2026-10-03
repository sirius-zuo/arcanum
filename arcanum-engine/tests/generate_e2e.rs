//! End-to-end Generate over the full ingestion harness.

#[path = "support/provenance_harness.rs"]
mod provenance_harness;

use arcanum_core::config::{ContextConfig, GenerateConfig};
use arcanum_core::traits::{
    ApproxCl100kCounter, ChunkMetadataStore, ScriptStep, ScriptedGenerator, StopReason,
};
use arcanum_core::types::{GenerateContextOptions, GenerateMode, GenerateRequest, GenerateStatus};
use arcanum_engine::audit::AuditLogger;
use arcanum_engine::auth::{ApiKeyClaims, AuthMiddleware};
use arcanum_engine::services::context::ContextService;
use arcanum_engine::services::generate::{GenerateService, GeneratorEntry};
use arcanum_graph::{GraphQueryPlanner, HopDecayScorer};
use arcanum_middleware::CircuitBreaker;
use arcanum_retrieval::{
    Bm25Retriever, GraphRetriever, OrchestratorMode, RetrievalOrchestrator, VectorRetriever,
};
use provenance_harness::*;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

#[tokio::test]
async fn generate_over_full_ingestion_maps_citations_to_registry() {
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
    let audit = Arc::new(AuditLogger::new());
    let context = Arc::new(ContextService::new(
        orchestrator,
        fx.chunk_metadata.clone(),
        None,
        Arc::new(ApproxCl100kCounter),
        ContextConfig::default(),
        Arc::new(AuthMiddleware::new("a-32-char-secret-for-testing-ok!")),
        audit.clone(),
        Arc::new(CircuitBreaker::new(
            "vector_store",
            5,
            Duration::from_secs(30),
        )),
    ));
    let generator = Arc::new(ScriptedGenerator::new(
        "m",
        vec![
            ScriptStep::Delta("Acme Corp builds rockets ".into()),
            ScriptStep::Delta("[P1][P9].".into()),
            ScriptStep::Done(StopReason::EndTurn),
        ],
    ));
    let mut generators = HashMap::new();
    generators.insert(
        "fake".to_string(),
        GeneratorEntry {
            generator,
            max_output_tokens: 1000,
            breaker: Arc::new(CircuitBreaker::new(
                "generator:fake",
                5,
                Duration::from_secs(30),
            )),
        },
    );
    let config = GenerateConfig {
        default_generator: Some("fake".into()),
        ..GenerateConfig::default()
    };
    let svc = GenerateService::new(context, Arc::new(generators), None, config, audit);
    let claims = ApiKeyClaims {
        user_id: "u1".into(),
        allowed_collections: vec![COLLECTION.into()],
        is_admin: false,
        exp: usize::MAX,
    };

    let resp = svc
        .generate(
            GenerateRequest {
                collection_id: COLLECTION.into(),
                mode: GenerateMode::Answer,
                query: Some("What does Acme Corp build?".into()),
                messages: None,
                generator: None,
                max_tokens: None,
                temperature: None,
                instructions: None,
                context: GenerateContextOptions {
                    token_budget: Some(600),
                    background_share: None,
                    candidate_k: None,
                },
                stream: false,
                verify: false,
            },
            &claims,
        )
        .await
        .unwrap();

    assert_eq!(resp.outcome.status, GenerateStatus::Ok);
    assert_eq!(resp.answer, "Acme Corp builds rockets [P1][P9].");
    assert_eq!(resp.outcome.unknown_refs, vec!["P9".to_string()]);
    assert_eq!(resp.outcome.citations.len(), 1);
    let c: &arcanum_core::types::generate::Citation = &resp.outcome.citations[0];
    assert_eq!(c.ref_id, "P1");
    assert_eq!(c.answer_spans, vec![(25, 29)]);
    assert_eq!(&resp.answer[25..29], "[P1]");
    let p1 = &resp.context.passages[0];
    assert_eq!(p1.ref_id, "P1");
    assert_eq!(c.document_id, p1.document_id);
    assert_eq!(c.version_num, p1.version_num);
    assert_eq!(c.offset_start, p1.offset_start);
    assert_eq!(c.offset_end, p1.offset_end);
    assert_eq!(c.chunk_ids, p1.chunk_ids);
    for id in &c.chunk_ids {
        assert!(fx.chunk_metadata.get(id).await.unwrap().is_some());
    }
}
