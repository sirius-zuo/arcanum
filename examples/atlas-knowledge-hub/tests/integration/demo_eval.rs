use crate::common::{ingest_and_wait, test_state};
use arcanum_core::types::ChunkId;
use atlas::demo::eval::first_relevant_rank;
use atlas::demo::{demo_router, OllamaProbe};
use atlas::samples::{load_manifest, GoldenQuery};
use axum::body::Body;
use axum::Router;
use http::{Request, StatusCode};
use serde_json::Value;
use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;
use tower::ServiceExt;

struct NoProbe;

#[async_trait::async_trait]
impl OllamaProbe for NoProbe {
    async fn tags(&self) -> Result<Vec<String>, String> {
        Ok(vec![])
    }
}

fn samples_dir() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("samples")
}

fn sample(rel: &str) -> Vec<u8> {
    std::fs::read(samples_dir().join(rel)).unwrap()
}

fn golden(query: &str, uri: &str) -> GoldenQuery {
    GoldenQuery {
        query: query.into(),
        relevant_source_uri: uri.into(),
    }
}

async fn post_eval(router: &Router, key: Option<&str>) -> (StatusCode, Value) {
    let mut req = Request::builder().method("POST").uri("/demo/eval");
    if let Some(k) = key {
        req = req.header("Authorization", format!("Bearer {k}"));
    }
    let resp = router
        .clone()
        .oneshot(req.body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

#[tokio::test]
async fn eval_reports_metrics_on_loaded_corpus() {
    let (state, _d) = test_state().await;
    for f in ["security-policy.md", "employee-handbook.md"] {
        let r = ingest_and_wait(&state, f, &sample(f), "standard").await;
        assert_eq!(format!("{:?}", r.status), "Succeeded", "{r:?}");
    }
    let mut manifest = load_manifest(&samples_dir()).unwrap();
    manifest.golden = vec![
        golden("security policy requirements", "security-policy.md"),
        golden("employee handbook leave policy", "employee-handbook.md"),
    ];
    let router = demo_router(state.clone(), Arc::new(manifest), Arc::new(NoProbe));

    let (st, body) = post_eval(&router, Some(&state.admin_key)).await;
    assert_eq!(st, StatusCode::OK, "{body}");
    let report = &body["report"];
    assert_eq!(report["num_queries"], 2);
    assert_eq!(report["k"], 5);
    for m in [
        "hit_rate_at_k",
        "mrr",
        "ndcg_at_k",
        "precision_at_k",
        "recall_at_k",
    ] {
        let v = report[m].as_f64().unwrap_or_else(|| panic!("{m} missing"));
        assert!((0.0..=1.0).contains(&v), "{m} = {v}");
    }
    assert!(report["hit_rate_at_k"].as_f64().unwrap() > 0.0);
    let queries = body["queries"].as_array().unwrap();
    assert_eq!(queries.len(), 2);
    for q in queries {
        assert!(q["first_relevant_rank"].is_number(), "{q}");
        assert!(q["query"].is_string() && q["relevant_source_uri"].is_string());
    }
}

#[tokio::test]
async fn eval_without_corpus_is_409() {
    let (state, _d) = test_state().await;
    let mut manifest = load_manifest(&samples_dir()).unwrap();
    manifest.golden = vec![golden("anything", "security-policy.md")];
    let router = demo_router(state.clone(), Arc::new(manifest), Arc::new(NoProbe));
    let (st, body) = post_eval(&router, Some(&state.admin_key)).await;
    assert_eq!(st, StatusCode::CONFLICT);
    assert_eq!(body["error"], "corpus not loaded");
}

#[tokio::test]
async fn eval_requires_key() {
    let (state, _d) = test_state().await;
    let manifest = load_manifest(&samples_dir()).unwrap();
    let router = demo_router(state, Arc::new(manifest), Arc::new(NoProbe));
    let (st, _) = post_eval(&router, None).await;
    assert_eq!(st, StatusCode::UNAUTHORIZED);
}

#[test]
fn rank_is_none_when_not_retrieved() {
    let ids: Vec<ChunkId> = (0..4).map(|_| ChunkId::default()).collect();
    let relevant: HashSet<ChunkId> = [ChunkId::default()].into_iter().collect();
    assert_eq!(first_relevant_rank(&ids, &relevant), None);
    assert_eq!(first_relevant_rank(&[], &relevant), None);

    let relevant: HashSet<ChunkId> = [ids[2].clone()].into_iter().collect();
    assert_eq!(first_relevant_rank(&ids, &relevant), Some(3));
}

#[tokio::test]
async fn superseded_version_chunks_do_not_matter() {
    let (state, _d) = test_state().await;
    for f in ["security-policy.md", "updates/security-policy.md"] {
        let r = ingest_and_wait(&state, "security-policy.md", &sample(f), "standard").await;
        assert_eq!(format!("{:?}", r.status), "Succeeded", "{r:?}");
    }
    let mut manifest = load_manifest(&samples_dir()).unwrap();
    manifest.golden = vec![golden("security policy requirements", "security-policy.md")];
    let router = demo_router(state.clone(), Arc::new(manifest), Arc::new(NoProbe));
    let (st, body) = post_eval(&router, Some(&state.admin_key)).await;
    assert_eq!(st, StatusCode::OK, "{body}");
    assert!(
        body["queries"][0]["first_relevant_rank"].is_number(),
        "{body}"
    );
}

#[tokio::test]
async fn empty_golden_is_409() {
    let (state, _d) = test_state().await;
    let mut manifest = load_manifest(&samples_dir()).unwrap();
    manifest.golden = vec![];
    let router = demo_router(state.clone(), Arc::new(manifest), Arc::new(NoProbe));
    let (st, body) = post_eval(&router, Some(&state.admin_key)).await;
    assert_eq!(st, StatusCode::CONFLICT);
    assert_eq!(body["error"], "no golden queries");
}
