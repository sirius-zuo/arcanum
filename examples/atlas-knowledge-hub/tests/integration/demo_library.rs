use crate::common::{ingest_and_wait, test_state};
use arcanum_core::types::ChunkBackend;
use atlas::demo::{demo_router, OllamaProbe};
use atlas::samples::load_manifest;
use atlas::AtlasState;
use axum::body::Body;
use axum::Router;
use http::{Request, StatusCode};
use serde_json::Value;
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

fn sample(rel: &str) -> Vec<u8> {
    std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("samples")
            .join(rel),
    )
    .unwrap()
}

async fn setup() -> (Arc<AtlasState>, Router, String, tempfile::TempDir) {
    let (state, dir) = test_state().await;
    let manifest = load_manifest(&Path::new(env!("CARGO_MANIFEST_DIR")).join("samples")).unwrap();
    let router = demo_router(state.clone(), Arc::new(manifest), Arc::new(NoProbe));
    let key = state.admin_key.clone();
    (state, router, key, dir)
}

async fn get(router: &Router, uri: &str, key: Option<&str>) -> (StatusCode, Value) {
    let mut req = Request::builder().uri(uri);
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
async fn library_lists_documents_with_versions() {
    let (state, router, key, _d) = setup().await;
    let r1 = ingest_and_wait(
        &state,
        "security-policy.md",
        &sample("security-policy.md"),
        "standard",
    )
    .await;
    assert_eq!(format!("{:?}", r1.status), "Succeeded", "{r1:?}");
    let r2 = ingest_and_wait(
        &state,
        "security-policy.md",
        &sample("updates/security-policy.md"),
        "standard",
    )
    .await;
    assert_eq!(format!("{:?}", r2.status), "Succeeded", "{r2:?}");

    let (st, lib) = get(&router, "/demo/library", Some(&key)).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(lib["collection"], "halcyon");
    let docs = lib["documents"].as_array().unwrap();
    assert_eq!(docs.len(), 1, "{lib}");
    let d = &docs[0];
    assert_eq!(d["source_uri"], "security-policy.md");
    assert!(d["chunks"].as_u64().unwrap() > 0);
    let vs = d["versions"].as_array().unwrap();
    assert_eq!(vs.len(), 2);
    assert_eq!(vs[0]["version_num"], 1);
    assert_eq!(vs[0]["status"], "Superseded");
    assert_eq!(vs[1]["version_num"], 2);
    assert_eq!(vs[1]["status"], "Active");
    assert!(chrono::DateTime::parse_from_rfc3339(vs[1]["ingested_at"].as_str().unwrap()).is_ok());
    assert!(!vs[1]["content_hash"].as_str().unwrap().is_empty());
    assert!(!vs[1]["snapshot_uri"].as_str().unwrap().is_empty());
    assert_ne!(vs[0]["content_hash"], vs[1]["content_hash"]);
}

#[tokio::test]
async fn document_text_matches_evidence_offsets() {
    let (state, router, key, _d) = setup().await;
    // Multibyte text so a char-index slice would land on the wrong bytes.
    let body = "# Café policy\n\nNaïve 日本語 rules apply to every laptop. Réseau access needs approval.\n\n\
                ## Second\n\nEmoji 🔒 are allowed in passwords, and so is 日本語.\n";
    ingest_and_wait(&state, "unicode.md", body.as_bytes(), "standard").await;
    ingest_and_wait(
        &state,
        "security-policy.md",
        &sample("security-policy.md"),
        "standard",
    )
    .await;

    let (_, lib) = get(&router, "/demo/library", Some(&key)).await;
    for doc in lib["documents"].as_array().unwrap() {
        let uri = doc["source_uri"].as_str().unwrap();
        let id = doc["document_id"].as_str().unwrap();
        let latest = doc["versions"].as_array().unwrap().last().unwrap()["version_num"]
            .as_u64()
            .unwrap() as u32;
        let (st, t) = get(
            &router,
            &format!("/demo/documents/{id}/versions/{latest}/text"),
            Some(&key),
        )
        .await;
        assert_eq!(st, StatusCode::OK, "{t}");
        assert_eq!(t["document_id"], id);
        assert_eq!(t["version_num"], latest);
        assert_eq!(t["source_uri"], uri);
        assert_eq!(t["status"], "Active");
        assert!(t["mime_type"].is_string());
        let text = t["text"].as_str().unwrap();

        let records: Vec<_> = state
            .registry
            .get_all()
            .await
            .into_iter()
            .filter(|r| {
                r.source_uri == uri
                    && r.collection_id == "halcyon"
                    && r.backend == ChunkBackend::Vector
                    && r.version_num == latest
            })
            .collect();
        assert!(!records.is_empty(), "no records for {uri}");
        assert_eq!(doc["chunks"].as_u64().unwrap() as usize, records.len());
        for r in records {
            let slice = &text.as_bytes()[r.offset_start..r.offset_end];
            assert_eq!(std::str::from_utf8(slice).unwrap(), r.text, "{uri}");
        }
    }
}

#[tokio::test]
async fn unknown_document_is_404() {
    let (state, router, key, _d) = setup().await;
    ingest_and_wait(
        &state,
        "security-policy.md",
        &sample("security-policy.md"),
        "standard",
    )
    .await;
    let (_, lib) = get(&router, "/demo/library", Some(&key)).await;
    let id = lib["documents"][0]["document_id"]
        .as_str()
        .unwrap()
        .to_string();

    let unknown = "00000000-0000-4000-8000-000000000000";
    let (st, _) = get(
        &router,
        &format!("/demo/documents/{unknown}/versions/1/text"),
        Some(&key),
    )
    .await;
    assert_eq!(st, StatusCode::NOT_FOUND);
    let (st, _) = get(
        &router,
        &format!("/demo/documents/{id}/versions/99/text"),
        Some(&key),
    )
    .await;
    assert_eq!(st, StatusCode::NOT_FOUND);
    let (st, b) = get(
        &router,
        "/demo/documents/not-a-uuid/versions/1/text",
        Some(&key),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
    assert!(b["error"].is_string());
}

#[tokio::test]
async fn library_requires_key() {
    let (_state, router, _key, _d) = setup().await;
    let (st, _) = get(&router, "/demo/library", None).await;
    assert_eq!(st, StatusCode::UNAUTHORIZED);
    let (st, _) = get(&router, "/demo/library", Some("bogus")).await;
    assert_eq!(st, StatusCode::UNAUTHORIZED);
    let id = "00000000-0000-4000-8000-000000000000";
    let (st, _) = get(
        &router,
        &format!("/demo/documents/{id}/versions/1/text"),
        None,
    )
    .await;
    assert_eq!(st, StatusCode::UNAUTHORIZED);
}
