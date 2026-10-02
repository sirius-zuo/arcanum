//! HTTP contract tests for the durable ingestion operation routes
//! (Plan 05, Task 4).
//!
//! Matrix: multipart metadata+payload submission; submission by retrievable
//! payload location; 202 new / 200 idempotent replay / 409 conflicting
//! idempotency; 404 unknown; query by ID and by idempotency_key (exactly one
//! query value); absent MIME hint; logical URI distinct from filename;
//! collection authorization; and idempotent removal by stable source URI.

use arcanum_core::config::ArcanumConfig;
use arcanum_engine::ArcanumEngine;
use arcanum_server::build_app;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use std::sync::Arc;
use tower::ServiceExt;

const BOUNDARY: &str = "ARCA-T4-BOUNDARY";

async fn test_engine() -> Arc<ArcanumEngine> {
    ArcanumEngine::builder()
        .auth_secret("a-32-char-secret-for-testing-ok!")
        .version_store(Arc::new(arcanum_core::traits::NoOpDocumentVersionStore))
        .build()
        .await
        .expect("engine must build")
}

async fn test_engine_with_config(config: ArcanumConfig) -> Arc<ArcanumEngine> {
    ArcanumEngine::builder()
        .config(config)
        .auth_secret("a-32-char-secret-for-testing-ok!")
        .version_store(Arc::new(arcanum_core::traits::NoOpDocumentVersionStore))
        .build()
        .await
        .expect("engine must build")
}

/// Build a `multipart/form-data` body with a `metadata` JSON part and an
/// optional `payload` binary part.
fn multipart_body(metadata: &str, payload: Option<&[u8]>) -> Vec<u8> {
    let mut body = Vec::new();
    body.extend_from_slice(format!("--{BOUNDARY}\r\n").as_bytes());
    body.extend_from_slice(b"Content-Disposition: form-data; name=\"metadata\"\r\n");
    body.extend_from_slice(b"Content-Type: application/json\r\n\r\n");
    body.extend_from_slice(metadata.as_bytes());
    body.extend_from_slice(b"\r\n");
    if let Some(payload) = payload {
        body.extend_from_slice(format!("--{BOUNDARY}\r\n").as_bytes());
        body.extend_from_slice(b"Content-Disposition: form-data; name=\"payload\"\r\n");
        body.extend_from_slice(b"Content-Type: application/octet-stream\r\n\r\n");
        body.extend_from_slice(payload);
        body.extend_from_slice(b"\r\n");
    }
    body.extend_from_slice(format!("--{BOUNDARY}--\r\n").as_bytes());
    body
}

fn metadata_json(
    idempotency_key: &str,
    source_uri: &str,
    collection: &str,
    mime_hint: Option<&str>,
    payload_locator: Option<&str>,
) -> String {
    let mut map = serde_json::Map::new();
    map.insert(
        "idempotency_key".to_string(),
        serde_json::json!(idempotency_key),
    );
    map.insert(
        "logical_source_uri".to_string(),
        serde_json::json!(source_uri),
    );
    if let Some(mime) = mime_hint {
        map.insert("mime_hint".to_string(), serde_json::json!(mime));
    }
    map.insert("collection_id".to_string(), serde_json::json!(collection));
    map.insert(
        "pipeline_configuration".to_string(),
        serde_json::json!({ "template": "standard" }),
    );
    if let Some(locator) = payload_locator {
        map.insert("payload_locator".to_string(), serde_json::json!(locator));
    }
    serde_json::Value::Object(map).to_string()
}

async fn read_json(resp: axum::response::Response) -> (StatusCode, serde_json::Value) {
    let status = resp.status();
    let body = resp.into_body();
    let bytes = http_body_util::BodyExt::collect(body)
        .await
        .expect("read response body")
        .to_bytes();
    let json = if bytes.is_empty() {
        serde_json::json!({})
    } else {
        serde_json::from_slice(&bytes).unwrap_or(serde_json::json!({}))
    };
    (status, json)
}

async fn post_submission(
    engine: &Arc<ArcanumEngine>,
    token: &str,
    metadata: &str,
    payload: Option<&[u8]>,
) -> (StatusCode, serde_json::Value) {
    let app = build_app(Some(engine.clone()));
    let body = multipart_body(metadata, payload);
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/ingestion-operations")
                .header("Authorization", format!("Bearer {token}"))
                .header(
                    "content-type",
                    format!("multipart/form-data; boundary={BOUNDARY}"),
                )
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    read_json(resp).await
}

fn unique_key(tag: &str) -> String {
    format!("t4-{tag}-{}", uuid::Uuid::new_v4())
}

#[tokio::test]
async fn multipart_submission_returns_202_new_operation() {
    let engine = test_engine().await;
    let token = engine.auth.generate_admin_key("tester");
    let meta = metadata_json(
        &unique_key("inline"),
        "s3://bucket/docs/guide.md",
        "col1",
        Some("text/markdown"),
        None,
    );

    let (status, json) = post_submission(&engine, &token, &meta, Some(b"# Hello world")).await;
    assert_eq!(
        status,
        StatusCode::ACCEPTED,
        "new inline submission must be 202: {json}"
    );
    let op_id = json["operation_id"].as_str().expect("operation_id present");
    assert_eq!(json["status"], "accepted");
    assert_eq!(
        json["resource"],
        format!("/api/v1/ingestion-operations/{op_id}")
    );
}

#[tokio::test]
async fn submission_by_payload_locator_returns_202() {
    let engine = test_engine().await;
    let token = engine.auth.generate_admin_key("tester");
    let locator = "file:///tmp/arcanum-test/operations/00000000-0000-0000-0000-000000000001";
    let meta = metadata_json(
        &unique_key("locator"),
        "s3://bucket/docs/remote.md",
        "col1",
        None,
        Some(locator),
    );

    let (status, json) = post_submission(&engine, &token, &meta, None).await;
    assert_eq!(
        status,
        StatusCode::ACCEPTED,
        "locator submission must be 202: {json}"
    );
    let op_id = json["operation_id"].as_str().expect("operation_id present");
    assert_eq!(json["status"], "accepted");

    // The durable locator is persisted on the submission and queryable.
    let app = build_app(Some(engine.clone()));
    let resp = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/api/v1/ingestion-operations/{op_id}"))
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, json) = read_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        json["submission"]["payload_locator"],
        serde_json::json!(locator)
    );
    assert!(
        json["submission"]["payload"].is_null(),
        "payload bytes are never surfaced"
    );
}

#[tokio::test]
async fn metadata_with_both_inline_payload_and_locator_is_rejected() {
    let engine = test_engine().await;
    let token = engine.auth.generate_admin_key("tester");
    let mut map = serde_json::Map::new();
    map.insert(
        "idempotency_key".to_string(),
        serde_json::json!(unique_key("both")),
    );
    map.insert(
        "logical_source_uri".to_string(),
        serde_json::json!("s3://bucket/docs/both.md"),
    );
    map.insert("collection_id".to_string(), serde_json::json!("col1"));
    map.insert(
        "pipeline_configuration".to_string(),
        serde_json::json!({ "template": "standard" }),
    );
    map.insert("payload".to_string(), serde_json::json!([1, 2, 3]));
    map.insert(
        "payload_locator".to_string(),
        serde_json::json!(
            "file:///tmp/arcanum-test/operations/00000000-0000-0000-0000-000000000001"
        ),
    );
    let meta = serde_json::Value::Object(map).to_string();

    let (status, json) = post_submission(&engine, &token, &meta, None).await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "both inline payload and payload_locator must be 400, not 409: {json}"
    );
}

#[tokio::test]
async fn idempotent_replay_returns_200_and_same_operation() {
    let engine = test_engine().await;
    let token = engine.auth.generate_admin_key("tester");
    let key = unique_key("replay");
    let meta = metadata_json(&key, "s3://bucket/docs/replay.md", "col1", None, None);
    let payload = b"identical bytes".to_vec();

    let (status1, json1) = post_submission(&engine, &token, &meta, Some(&payload)).await;
    assert_eq!(status1, StatusCode::ACCEPTED);

    let (status2, json2) = post_submission(&engine, &token, &meta, Some(&payload)).await;
    assert_eq!(
        status2,
        StatusCode::OK,
        "identical replay must be 200: {json2}"
    );
    assert_eq!(
        json1["operation_id"], json2["operation_id"],
        "replay returns the ORIGINAL operation"
    );
}

#[tokio::test]
async fn conflicting_idempotency_returns_409() {
    let engine = test_engine().await;
    let token = engine.auth.generate_admin_key("tester");
    let key = unique_key("conflict");
    let meta = metadata_json(&key, "s3://bucket/docs/conflict.md", "col1", None, None);

    let (status1, _) = post_submission(&engine, &token, &meta, Some(b"first bytes")).await;
    assert_eq!(status1, StatusCode::ACCEPTED);

    let (status2, json2) = post_submission(&engine, &token, &meta, Some(b"DIFFERENT bytes")).await;
    assert_eq!(
        status2,
        StatusCode::CONFLICT,
        "different submission under same key must be 409: {json2}"
    );
}

#[tokio::test]
async fn get_unknown_operation_returns_404() {
    let engine = test_engine().await;
    let token = engine.auth.generate_admin_key("tester");
    let app = build_app(Some(engine.clone()));
    let resp = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!(
                    "/api/v1/ingestion-operations/{}",
                    uuid::Uuid::new_v4()
                ))
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn query_by_id_returns_canonical_operation() {
    let engine = test_engine().await;
    let token = engine.auth.generate_admin_key("tester");
    let key = unique_key("byid");
    let meta = metadata_json(&key, "s3://bucket/docs/byid.md", "colA", None, None);

    let (status, json) = post_submission(&engine, &token, &meta, Some(b"content")).await;
    assert_eq!(status, StatusCode::ACCEPTED);
    let op_id = json["operation_id"].as_str().unwrap().to_string();

    let app = build_app(Some(engine.clone()));
    let resp = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/api/v1/ingestion-operations/{op_id}"))
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, json) = read_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["operation_id"].as_str(), Some(op_id.as_str()));
    assert_eq!(json["submission"]["collection_id"], "colA");
    assert_eq!(json["submission"]["idempotency_key"], key);
    assert_eq!(json["status"], "Accepted");
    assert!(
        json["terminal_report"].is_null(),
        "no worker ran in this test, so no terminal report"
    );
}

#[tokio::test]
async fn query_by_idempotency_key_requires_exactly_one_value() {
    let engine = test_engine().await;
    let token = engine.auth.generate_admin_key("tester");
    let key = unique_key("bykey");
    let meta = metadata_json(&key, "s3://bucket/docs/bykey.md", "col1", None, None);
    let (status, _) = post_submission(&engine, &token, &meta, Some(b"content")).await;
    assert_eq!(status, StatusCode::ACCEPTED);

    let app = build_app(Some(engine.clone()));
    let uri = format!("/api/v1/ingestion-operations?idempotency_key={key}");

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(&uri)
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, json) = read_json(resp).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "single idempotency_key must resolve: {json}"
    );
    assert_eq!(json["submission"]["idempotency_key"], key);

    // Zero query values → 400.
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/ingestion-operations")
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::BAD_REQUEST,
        "missing idempotency_key must be 400"
    );

    // Multiple query values → 400.
    let resp = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!(
                    "/api/v1/ingestion-operations?idempotency_key={key}&idempotency_key=other"
                ))
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::BAD_REQUEST,
        "multiple idempotency_key values must be 400"
    );
}

#[tokio::test]
async fn absent_mime_hint_is_optional() {
    let engine = test_engine().await;
    let token = engine.auth.generate_admin_key("tester");
    let key = unique_key("nomime");
    let meta = metadata_json(&key, "s3://bucket/docs/nomime.md", "col1", None, None);

    let (status, json) = post_submission(&engine, &token, &meta, Some(b"content")).await;
    assert_eq!(status, StatusCode::ACCEPTED);
    let op_id = json["operation_id"].as_str().unwrap();

    let app = build_app(Some(engine.clone()));
    let resp = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/api/v1/ingestion-operations/{op_id}"))
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, json) = read_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        json["submission"]["mime_hint"].is_null(),
        "MIME hint may be absent: {json}"
    );
}

#[tokio::test]
async fn logical_source_uri_stays_distinct_from_filename() {
    let engine = test_engine().await;
    let token = engine.auth.generate_admin_key("tester");
    let key = unique_key("uri");
    // The logical URI is stable and is never replaced by the upload filename.
    let meta = metadata_json(
        &key,
        "s3://bucket/canonical/docs/guide.md",
        "col1",
        None,
        None,
    );

    let (status, json) = post_submission(&engine, &token, &meta, Some(b"# Guide")).await;
    assert_eq!(status, StatusCode::ACCEPTED);
    let op_id = json["operation_id"].as_str().unwrap();

    let app = build_app(Some(engine.clone()));
    let resp = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/api/v1/ingestion-operations/{op_id}"))
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, json) = read_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        json["submission"]["logical_source_uri"], "s3://bucket/canonical/docs/guide.md",
        "logical source URI must be preserved verbatim"
    );
}

#[tokio::test]
async fn collection_authorization_is_enforced() {
    let engine = test_engine().await;
    // Non-admin key scoped to col-a only.
    let token = engine
        .auth
        .generate_api_key("tester", vec!["col-a".to_string()]);

    // Submission to a collection outside the caller's scope → 403.
    let meta_denied = metadata_json(&unique_key("authz"), "s3://b/x.md", "col-b", None, None);
    let (status, json) = post_submission(&engine, &token, &meta_denied, Some(b"x")).await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "cross-collection submission must be 403: {json}"
    );

    // Submission to an allowed collection → 202.
    let meta_allowed = metadata_json(&unique_key("authz"), "s3://b/y.md", "col-a", None, None);
    let (status, json) = post_submission(&engine, &token, &meta_allowed, Some(b"y")).await;
    assert_eq!(
        status,
        StatusCode::ACCEPTED,
        "allowed collection must be 202: {json}"
    );
    let op_id = json["operation_id"].as_str().unwrap().to_string();

    // Querying that operation with the same scoped key → 200.
    let app = build_app(Some(engine.clone()));
    let resp = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/api/v1/ingestion-operations/{op_id}"))
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn source_removal_is_idempotent_by_stable_uri() {
    let engine = test_engine().await;
    let token = engine.auth.generate_admin_key("tester");
    let uri = "s3://bucket/docs/remove.md";
    let url = format!("/api/v1/collections/col1/sources?source_uri={uri}");

    let app = build_app(Some(engine.clone()));
    let req = |uri: String| {
        Request::builder()
            .method("DELETE")
            .uri(uri)
            .header("Authorization", format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap()
    };

    let resp = app.clone().oneshot(req(url.clone())).await.unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::NO_CONTENT,
        "first removal must succeed"
    );

    let resp = app.oneshot(req(url)).await.unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::NO_CONTENT,
        "repeated removal for an absent source is a no-op success"
    );
}

#[tokio::test]
async fn new_routes_require_auth() {
    let app = build_app(None);

    // Multipart submission without a bearer → 401.
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/ingestion-operations")
                .header(
                    "content-type",
                    format!("multipart/form-data; boundary={BOUNDARY}"),
                )
                .body(Body::from(multipart_body("{}", Some(b"x"))))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

    // GET by id → 401.
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!(
                    "/api/v1/ingestion-operations/{}",
                    uuid::Uuid::new_v4()
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

    // DELETE sources → 401.
    let resp = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/v1/collections/col1/sources?source_uri=x")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn payload_over_max_upload_bytes_is_rejected() {
    let mut config = ArcanumConfig::default();
    config.ingestion.max_upload_bytes = 32; // tiny limit for the test
    let engine = test_engine_with_config(config).await;
    let token = engine.auth.generate_admin_key("tester");
    let meta = metadata_json(&unique_key("large"), "s3://b/large.md", "col1", None, None);

    let (status, json) = post_submission(
        &engine,
        &token,
        &meta,
        Some(b"this payload is definitely more than 32 bytes"),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::PAYLOAD_TOO_LARGE,
        "oversized payload must be 413: {json}"
    );
}
