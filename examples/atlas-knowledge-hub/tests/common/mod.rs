//! Shared helpers for the integration tests: an offline `AtlasState` and a blocking ingest.
#![allow(dead_code)]

use arcanum_core::traits::{Embedder, Generator, ScriptStep, ScriptedGenerator, StopReason};
use arcanum_core::types::{
    CollectionId, IngestionReport, IngestionSubmission, OperationStatus, Vector,
};
use atlas::engine_setup::COLLECTION;
use atlas::{build_state, AtlasState, ModelDeps, Settings};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Deterministic, offline embedder: a byte-sum histogram over 8 dimensions.
pub struct FakeEmbedder;

#[async_trait::async_trait]
impl Embedder for FakeEmbedder {
    async fn embed(&self, texts: Vec<String>) -> arcanum_core::Result<Vec<Vector>> {
        Ok(texts
            .iter()
            .map(|t| {
                let mut v = vec![0.0f32; 8];
                for (i, b) in t.bytes().enumerate() {
                    v[i % 8] += b as f32;
                }
                Vector(v)
            })
            .collect())
    }
    fn dimension(&self) -> usize {
        8
    }
}

/// Offline model dependencies: fake embedder, no enricher, one scripted generator `local`
/// that is also the judge.
pub fn models() -> ModelDeps {
    let gen: Arc<dyn Generator> = Arc::new(ScriptedGenerator::new(
        "m",
        vec![
            ScriptStep::Delta("ok".into()),
            ScriptStep::Done(StopReason::EndTurn),
        ],
    ));
    ModelDeps {
        embedder: Arc::new(FakeEmbedder),
        enricher: None,
        generators: vec![("local".into(), gen, 512)],
        default_generator: "local".into(),
        judge: Some("local".into()),
    }
}

/// An offline state over a fresh temp data dir. Keep the `TempDir` alive for the test.
pub async fn test_state() -> (Arc<AtlasState>, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let state = build_state(Settings::for_tests(dir.path().join("data")), models())
        .await
        .unwrap();
    (Arc::new(state), dir)
}

/// Submits `bytes` as `source_uri` into `halcyon` and waits (30 s) for the operation to reach
/// a terminal state. `template` is the pipeline template name, for example `standard`.
pub async fn ingest_and_wait(
    state: &AtlasState,
    source_uri: &str,
    bytes: &[u8],
    template: &str,
) -> IngestionReport {
    let ingestion = &state.engine.ingestion;
    let submission = IngestionSubmission {
        idempotency_key: format!("{source_uri}:{}", uuid_like(bytes)),
        logical_source_uri: source_uri.to_string(),
        mime_hint: None,
        collection_id: CollectionId(COLLECTION.into()),
        pipeline_configuration: serde_json::json!({ "template": template }),
        payload: Some(bytes.to_vec()),
        payload_locator: None,
    };
    let (op_id, _) = ingestion
        .submit_operation(submission, false, "atlas")
        .await
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let op = ingestion.operations().get(&op_id).await.unwrap().unwrap();
        if matches!(
            op.status,
            OperationStatus::Succeeded | OperationStatus::Failed
        ) {
            return op.terminal_report.expect("terminal report");
        }
        assert!(Instant::now() < deadline, "operation timed out: {op:?}");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// Cheap content fingerprint so re-ingesting different bytes gets a different idempotency key.
fn uuid_like(bytes: &[u8]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut h);
    h.finish()
}
