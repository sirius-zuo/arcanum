//! End-to-end Verify over the full ingestion harness.

#[path = "support/provenance_harness.rs"]
mod provenance_harness;

use arcanum_core::config::VerifyConfig;
use arcanum_core::traits::{ApproxCl100kCounter, ScriptStep, ScriptedGenerator, StopReason};
use arcanum_core::types::{
    ChunkBackend, ChunkMetadataRecord, OverallVerdict, PassageRef, SentenceVerdict, VerifyRequest,
};
use arcanum_engine::audit::AuditLogger;
use arcanum_engine::auth::{ApiKeyClaims, AuthMiddleware};
use arcanum_engine::services::generate::GeneratorEntry;
use arcanum_engine::services::verify::VerifyService;
use arcanum_middleware::CircuitBreaker;
use provenance_harness::*;
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

#[tokio::test]
async fn verify_over_full_ingestion_traces_evidence_to_source() {
    let fx = ingest_fixture().await;
    let mut chunks: Vec<ChunkMetadataRecord> = fx
        .chunk_metadata
        .get_all()
        .await
        .into_iter()
        .filter(|r| r.backend == ChunkBackend::Vector && r.document_id == fx.doc_a_id)
        .collect();
    chunks.sort_by_key(|r| r.offset_start);
    assert!(
        chunks.len() >= 4,
        "need at least 4 chunks, got {}",
        chunks.len()
    );
    let p1: Vec<&ChunkMetadataRecord> = chunks[..2].iter().collect();
    let p2: Vec<&ChunkMetadataRecord> = chunks[chunks.len() - 2..].iter().collect();

    let judge_reply = json!({"sentences": [
        {"id": 1, "kind": "claim", "claims": [{"text": "Acme Corp builds rockets for orbital delivery",
            "support": [{"ref": "P1", "quote": "builds rockets for orbital delivery"}]}]},
        {"id": 2, "kind": "claim", "claims": [{"text": "Bob mentors new hires on safe engine testing",
            "support": [{"ref": "P2", "quote": "Bob mentors new hires on safe engine testing"}]}]},
        {"id": 3, "kind": "claim", "claims": [{"text": "Acme Corp is based in Berlin", "support": []}]},
        {"id": 4, "kind": "no_claim", "claims": []},
    ]})
    .to_string();

    let auth = Arc::new(AuthMiddleware::new("a-32-char-secret-for-testing-ok!"));
    let judge = Arc::new(ScriptedGenerator::new(
        "judge-model",
        vec![
            ScriptStep::Delta(judge_reply),
            ScriptStep::Done(StopReason::EndTurn),
        ],
    ));
    let mut generators = HashMap::new();
    generators.insert(
        "judge".to_string(),
        GeneratorEntry {
            generator: judge,
            max_output_tokens: 2000,
            breaker: Arc::new(CircuitBreaker::new(
                "judge:judge",
                5,
                Duration::from_secs(30),
            )),
        },
    );
    let config = VerifyConfig {
        judge: Some("judge".into()),
        ..VerifyConfig::default()
    };
    let svc = VerifyService::new(
        fx.chunk_metadata.clone(),
        fx.version_store.clone(),
        Arc::new(generators),
        Arc::new(ApproxCl100kCounter),
        config,
        auth,
        Arc::new(AuditLogger::new()),
    );
    let claims = ApiKeyClaims {
        user_id: "u1".into(),
        allowed_collections: vec![COLLECTION.into()],
        is_admin: false,
        exp: usize::MAX,
    };

    let resp = svc
        .verify(
            VerifyRequest {
                collection_id: COLLECTION.into(),
                answer: "Acme Corp builds rockets for orbital delivery [P1]. Bob mentors new hires on safe engine testing [P1]. Acme Corp is based in Berlin [P1]. Hope this helps!".into(),
                passages: vec![
                    PassageRef {
                        ref_id: "P1".into(),
                        chunk_ids: p1.iter().map(|r| r.chunk_id.clone()).collect(),
                    },
                    PassageRef {
                        ref_id: "P2".into(),
                        chunk_ids: p2.iter().map(|r| r.chunk_id.clone()).collect(),
                    },
                ],
                judge: None,
                strict_citations: false,
            },
            &claims,
        )
        .await
        .unwrap();

    let verdicts: Vec<SentenceVerdict> = resp.sentences.iter().map(|s| s.verdict).collect();
    assert_eq!(
        verdicts,
        vec![
            SentenceVerdict::Supported,
            SentenceVerdict::Miscited,
            SentenceVerdict::Unsupported,
            SentenceVerdict::NoClaim
        ]
    );
    assert_eq!(resp.verdict, OverallVerdict::Fail);

    let mut seen = 0;
    for e in resp
        .sentences
        .iter()
        .flat_map(|s| &s.claims)
        .flat_map(|c| &c.evidence)
    {
        seen += 1;
        assert!(e.quote_matched);
        assert_eq!(e.version_status, "active");
        assert_eq!(&fx.doc_a_text[e.offset_start..e.offset_end], e.quote);
        let passage = if e.ref_id == "P1" { &p1 } else { &p2 };
        let owner = passage
            .iter()
            .find(|r| r.offset_start <= e.offset_start && e.offset_start < r.offset_end)
            .expect("a chunk contains the evidence start");
        assert_eq!(e.chunk_id, owner.chunk_id);
    }
    assert!(seen >= 2, "expected evidence for sentences 1 and 2");
}
