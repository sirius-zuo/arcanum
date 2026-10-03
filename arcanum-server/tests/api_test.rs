use arcanum_server::build_app;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

async fn get(uri: &str) -> StatusCode {
    let app = build_app(None);
    let req = Request::builder().uri(uri).body(Body::empty()).unwrap();
    app.oneshot(req).await.unwrap().status()
}

async fn post_json(uri: &str, body: serde_json::Value) -> StatusCode {
    let app = build_app(None);
    let req = Request::builder()
        .method("POST")
        .uri(uri)
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();
    app.oneshot(req).await.unwrap().status()
}

#[tokio::test]
async fn test_health_endpoint_returns_200() {
    assert_eq!(get("/health").await, StatusCode::OK);
}

#[tokio::test]
async fn test_ready_endpoint_returns_200() {
    assert_eq!(get("/ready").await, StatusCode::OK);
}

#[tokio::test]
async fn test_search_requires_auth() {
    let status = post_json(
        "/api/v1/search",
        serde_json::json!({ "query": "test", "collection_id": "docs" }),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_context_requires_auth() {
    let status = post_json(
        "/api/v1/context",
        serde_json::json!({ "collection_id": "docs", "query": "test" }),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

/// POSTs to /api/v1/context as an authenticated admin on an engine built
/// without a database, so it has no chunk registry.
async fn post_context_authed(body: serde_json::Value) -> (StatusCode, serde_json::Value) {
    use arcanum_core::traits::NoOpDocumentVersionStore;
    use arcanum_engine::ArcanumEngine;
    use std::sync::Arc;
    let engine = ArcanumEngine::builder()
        .auth_secret("a-32-char-secret-for-testing-ok!")
        .version_store(Arc::new(NoOpDocumentVersionStore))
        .build()
        .await
        .unwrap();
    let token = engine.auth.generate_admin_key("tester");
    let app = build_app(Some(engine));
    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/context")
        .header("Authorization", format!("Bearer {token}"))
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    (status, serde_json::from_slice(&bytes).unwrap())
}

#[tokio::test]
async fn test_context_unknown_role_is_400() {
    let (status, _) = post_context_authed(serde_json::json!({
        "collection_id": "docs",
        "messages": [{ "role": "system", "content": "hi" }],
    }))
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_context_without_chunk_registry_is_503() {
    let (status, body) =
        post_context_authed(serde_json::json!({ "collection_id": "docs", "query": "test" })).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["error"], "context requires a chunk registry");
}

#[tokio::test]
async fn test_ws_route_exists() {
    // Without upgrade headers → 400, not 404.
    let status = get("/ws/events").await;
    assert_ne!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_generate_requires_auth() {
    let status = post_json(
        "/api/v1/generate",
        serde_json::json!({ "collection_id": "docs", "query": "test" }),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

/// POSTs to /api/v1/generate as an authenticated admin. With `generator` the
/// engine has a chunk registry, an empty BM25 index and a scripted generator;
/// without it, neither. Returns status, content type and raw body.
async fn post_generate_authed(
    body: serde_json::Value,
    generator: bool,
) -> (StatusCode, String, String) {
    use arcanum_core::traits::{InMemoryChunkMetadataStore, NoOpDocumentVersionStore};
    use arcanum_core::traits::{ScriptStep, ScriptedGenerator, StopReason};
    use arcanum_engine::ArcanumEngine;
    use std::sync::Arc;
    let dir = tempfile::tempdir().unwrap();
    let mut builder = ArcanumEngine::builder()
        .auth_secret("a-32-char-secret-for-testing-ok!")
        .version_store(Arc::new(NoOpDocumentVersionStore));
    if generator {
        let bm25 = Arc::new(arcanum_vector::Bm25Index::new(dir.path().to_str().unwrap()).unwrap());
        let fake = Arc::new(ScriptedGenerator::new(
            "fake-model",
            vec![ScriptStep::Done(StopReason::EndTurn)],
        ));
        builder = builder
            .bm25_index(bm25)
            .chunk_metadata_store(Arc::new(InMemoryChunkMetadataStore::new()))
            .generator("fake", fake, 100);
    }
    let mut cfg = arcanum_core::ArcanumConfig::default();
    if generator {
        cfg.generate.default_generator = Some("fake".into());
    }
    let engine = builder.config(cfg).build().await.unwrap();
    let token = engine.auth.generate_admin_key("tester");
    let app = build_app(Some(engine));
    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/generate")
        .header("Authorization", format!("Bearer {token}"))
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let status = resp.status();
    let ct = resp
        .headers()
        .get("content-type")
        .map(|v| v.to_str().unwrap().to_string())
        .unwrap_or_default();
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    (status, ct, String::from_utf8(bytes.to_vec()).unwrap())
}

#[tokio::test]
async fn test_generate_unconfigured_is_503() {
    let (status, _, body) = post_generate_authed(
        serde_json::json!({ "collection_id": "docs", "query": "t" }),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    let v: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(
        v["error"],
        "generation requires a configured generator and a chunk registry"
    );
}

#[tokio::test]
async fn test_generate_bad_mode_is_400() {
    let (status, _, _) = post_generate_authed(
        serde_json::json!({ "collection_id": "docs", "query": "t", "mode": "poem" }),
        true,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_generate_no_context_json_and_sse_agree() {
    let (status, ct, body) = post_generate_authed(
        serde_json::json!({ "collection_id": "docs", "query": "t" }),
        true,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(ct.starts_with("application/json"), "{ct}");
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(json["status"], "no_context");
    assert_eq!(
        json["answer"],
        "No relevant information was found in the collection."
    );

    let (status, ct, body) = post_generate_authed(
        serde_json::json!({ "collection_id": "docs", "query": "t", "stream": true }),
        true,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(ct.starts_with("text/event-stream"), "{ct}");
    let events: Vec<(String, serde_json::Value)> = body
        .split("\n\n")
        .filter(|b| !b.trim().is_empty())
        .map(|block| {
            let name = block
                .lines()
                .find_map(|l| l.strip_prefix("event:"))
                .unwrap()
                .trim()
                .to_string();
            let data = block
                .lines()
                .find_map(|l| l.strip_prefix("data:"))
                .unwrap()
                .trim();
            (name, serde_json::from_str(data).unwrap())
        })
        .collect();
    let names: Vec<&str> = events.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(names, ["context", "delta", "done"]);
    assert_eq!(events[1].1["text"], json["answer"]);
    let mut expected = json.clone();
    let obj = expected.as_object_mut().unwrap();
    obj.remove("answer");
    obj.remove("context");
    assert_eq!(events[2].1, expected);
}

#[tokio::test]
async fn test_verify_requires_auth() {
    let status = post_json(
        "/api/v1/verify",
        serde_json::json!({ "collection_id": "docs", "answer": "a", "passages": [] }),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

/// POSTs to /api/v1/verify as an authenticated admin on an engine built
/// without a verifier.
async fn post_verify_authed(body: serde_json::Value) -> (StatusCode, serde_json::Value) {
    use arcanum_core::traits::NoOpDocumentVersionStore;
    use arcanum_engine::ArcanumEngine;
    use std::sync::Arc;
    let engine = ArcanumEngine::builder()
        .auth_secret("a-32-char-secret-for-testing-ok!")
        .version_store(Arc::new(NoOpDocumentVersionStore))
        .build()
        .await
        .unwrap();
    let token = engine.auth.generate_admin_key("tester");
    let app = build_app(Some(engine));
    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/verify")
        .header("Authorization", format!("Bearer {token}"))
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    (status, serde_json::from_slice(&bytes).unwrap())
}

#[tokio::test]
async fn test_verify_unconfigured_is_503() {
    let (status, body) = post_verify_authed(
        serde_json::json!({ "collection_id": "docs", "answer": "a", "passages": [] }),
    )
    .await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(
        body["error"],
        arcanum_engine::services::verify::VERIFY_UNAVAILABLE
    );
}

#[tokio::test]
async fn test_verify_bad_body_is_400() {
    let (status, _) =
        post_verify_authed(serde_json::json!({ "collection_id": "docs", "passages": [] })).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}
