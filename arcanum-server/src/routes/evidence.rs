use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    Json,
};
use std::sync::Arc;
use arcanum_engine::ArcanumEngine;
use crate::routes::auth::validate_bearer;

pub async fn get_chunk_evidence(
    headers:             HeaderMap,
    State(engine):       State<Option<Arc<ArcanumEngine>>>,
    Path(chunk_id_str):  Path<String>,
) -> impl IntoResponse {
    if let Err(e) = validate_bearer(&headers, &engine) { return e.into_response(); }
    let eng = engine.as_ref().unwrap();
    let Some(resolver) = &eng.evidence else {
        return (StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({"error": "evidence resolver not configured"}))).into_response();
    };
    let chunk_id = parse_chunk_id(&chunk_id_str);
    let chunk_id = match chunk_id {
        Ok(id) => id,
        Err(e) => return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": e}))).into_response(),
    };
    match resolver.resolve_chunk(&chunk_id).await {
        Ok(chain) => (StatusCode::OK, Json(chain)).into_response(),
        Err(arcanum_core::ArcanumError::NotFound(_)) =>
            (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "not found"}))).into_response(),
        Err(e) =>
            (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e.to_string()}))).into_response(),
    }
}

pub async fn get_tree_node_evidence(
    headers:            HeaderMap,
    State(engine):      State<Option<Arc<ArcanumEngine>>>,
    Path(node_id_str):  Path<String>,
) -> impl IntoResponse {
    if let Err(e) = validate_bearer(&headers, &engine) { return e.into_response(); }
    let eng = engine.as_ref().unwrap();
    let Some(resolver) = &eng.evidence else {
        return (StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({"error": "evidence resolver not configured"}))).into_response();
    };
    let node_id = parse_tree_node_id(&node_id_str);
    let node_id = match node_id {
        Ok(id) => id,
        Err(e) => return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": e}))).into_response(),
    };
    match resolver.resolve_tree_node(&node_id).await {
        Ok(chain) => (StatusCode::OK, Json(chain)).into_response(),
        Err(arcanum_core::ArcanumError::NotFound(_)) =>
            (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "not found"}))).into_response(),
        Err(e) =>
            (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e.to_string()}))).into_response(),
    }
}

pub async fn get_entity_evidence(
    headers:              HeaderMap,
    State(engine):        State<Option<Arc<ArcanumEngine>>>,
    Path(entity_id_str):  Path<String>,
) -> impl IntoResponse {
    if let Err(e) = validate_bearer(&headers, &engine) { return e.into_response(); }
    let eng = engine.as_ref().unwrap();
    let Some(resolver) = &eng.evidence else {
        return (StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({"error": "evidence resolver not configured"}))).into_response();
    };
    let entity_id = parse_entity_id(&entity_id_str);
    let entity_id = match entity_id {
        Ok(id) => id,
        Err(e) => return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": e}))).into_response(),
    };
    match resolver.resolve_entity(&entity_id).await {
        Ok(chain) => (StatusCode::OK, Json(chain)).into_response(),
        Err(arcanum_core::ArcanumError::NotFound(_)) =>
            (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "not found"}))).into_response(),
        Err(e) =>
            (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e.to_string()}))).into_response(),
    }
}

pub async fn get_relation_evidence(
    headers:       HeaderMap,
    State(engine): State<Option<Arc<ArcanumEngine>>>,
    Path((source_id_str, relation_type, target_id_str)): Path<(String, String, String)>,
) -> impl IntoResponse {
    if let Err(e) = validate_bearer(&headers, &engine) { return e.into_response(); }
    let eng = engine.as_ref().unwrap();
    let Some(resolver) = &eng.evidence else {
        return (StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({"error": "evidence resolver not configured"}))).into_response();
    };
    let (source_id, target_id) = match (parse_entity_id(&source_id_str), parse_entity_id(&target_id_str)) {
        (Ok(s), Ok(t)) => (s, t),
        _ => return (StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "invalid UUID in path"}))).into_response(),
    };
    match resolver.resolve_relation(&source_id, &relation_type, &target_id).await {
        Ok(chain) => (StatusCode::OK, Json(chain)).into_response(),
        Err(arcanum_core::ArcanumError::NotFound(_)) =>
            (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "not found"}))).into_response(),
        Err(e) =>
            (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e.to_string()}))).into_response(),
    }
}

fn parse_chunk_id(s: &str) -> std::result::Result<arcanum_core::types::ChunkId, String> {
    uuid::Uuid::parse_str(s)
        .map(arcanum_core::types::ChunkId)
        .map_err(|_| format!("invalid chunk UUID: {s}"))
}

fn parse_tree_node_id(s: &str) -> std::result::Result<arcanum_core::types::TreeNodeId, String> {
    uuid::Uuid::parse_str(s)
        .map(arcanum_core::types::TreeNodeId)
        .map_err(|_| format!("invalid tree node UUID: {s}"))
}

fn parse_entity_id(s: &str) -> std::result::Result<arcanum_core::types::EntityId, String> {
    uuid::Uuid::parse_str(s)
        .map(arcanum_core::types::EntityId)
        .map_err(|_| format!("invalid entity UUID: {s}"))
}

#[cfg(test)]
mod tests {
    use crate::build_app;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    #[tokio::test]
    async fn evidence_chunk_requires_auth() {
        let app = build_app(None);
        let resp = app.oneshot(
            Request::builder()
                .uri("/evidence/chunk/00000000-0000-0000-0000-000000000001")
                .body(Body::empty())
                .unwrap()
        ).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn evidence_tree_node_requires_auth() {
        let app = build_app(None);
        let resp = app.oneshot(
            Request::builder()
                .uri("/evidence/tree-node/00000000-0000-0000-0000-000000000001")
                .body(Body::empty())
                .unwrap()
        ).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn evidence_entity_requires_auth() {
        let app = build_app(None);
        let resp = app.oneshot(
            Request::builder()
                .uri("/evidence/entity/00000000-0000-0000-0000-000000000001")
                .body(Body::empty())
                .unwrap()
        ).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn evidence_relation_requires_auth() {
        let app = build_app(None);
        let resp = app.oneshot(
            Request::builder()
                .uri("/evidence/relation/00000000-0000-0000-0000-000000000001/SIGNED/00000000-0000-0000-0000-000000000002")
                .body(Body::empty())
                .unwrap()
        ).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    /// Engine present and authenticated, but no EvidenceResolver configured — this is the
    /// common deployment state today (nothing wires DefaultEvidenceResolver in), and was
    /// previously untested.
    async fn build_authed_app_without_evidence() -> (axum::Router, String) {
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
        (build_app(Some(engine)), token)
    }

    #[tokio::test]
    async fn evidence_chunk_returns_503_when_resolver_not_configured() {
        let (app, token) = build_authed_app_without_evidence().await;
        let resp = app.oneshot(
            Request::builder()
                .uri("/evidence/chunk/00000000-0000-0000-0000-000000000001")
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap()
        ).await.unwrap();
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    /// A resolver that's present (so the route gets past its 503 check) but whose methods
    /// are never expected to be called in this test — a malformed UUID is rejected by
    /// parse_chunk_id before resolve_chunk is ever invoked.
    struct FakeResolver;

    #[async_trait::async_trait]
    impl arcanum_core::traits::EvidenceResolver for FakeResolver {
        async fn resolve_chunk(&self, _: &arcanum_core::types::ChunkId) -> arcanum_core::Result<arcanum_core::types::ProofChain> {
            Err(arcanum_core::ArcanumError::NotFound("unused in this test".into()))
        }
        async fn resolve_tree_node(&self, _: &arcanum_core::types::TreeNodeId) -> arcanum_core::Result<arcanum_core::types::ProofChain> {
            Err(arcanum_core::ArcanumError::NotFound("unused in this test".into()))
        }
        async fn resolve_entity(&self, _: &arcanum_core::types::EntityId) -> arcanum_core::Result<arcanum_core::types::ProofChain> {
            Err(arcanum_core::ArcanumError::NotFound("unused in this test".into()))
        }
        async fn resolve_relation(
            &self,
            _: &arcanum_core::types::EntityId,
            _: &str,
            _: &arcanum_core::types::EntityId,
        ) -> arcanum_core::Result<arcanum_core::types::ProofChain> {
            Err(arcanum_core::ArcanumError::NotFound("unused in this test".into()))
        }
    }

    #[tokio::test]
    async fn evidence_chunk_returns_400_for_malformed_uuid() {
        use arcanum_core::traits::NoOpDocumentVersionStore;
        use arcanum_engine::ArcanumEngine;
        use std::sync::Arc;
        let engine = ArcanumEngine::builder()
            .auth_secret("a-32-char-secret-for-testing-ok!")
            .version_store(Arc::new(NoOpDocumentVersionStore))
            .evidence(Arc::new(FakeResolver))
            .build()
            .await
            .unwrap();
        let token = engine.auth.generate_admin_key("tester");
        let app = build_app(Some(engine));

        let resp = app.oneshot(
            Request::builder()
                .uri("/evidence/chunk/not-a-uuid")
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap()
        ).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }
}
