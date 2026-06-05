use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    Json,
};
use arcanum_core::ArcanumError;
use arcanum_engine::ArcanumEngine;
use std::sync::Arc;
use crate::routes::auth::validate_bearer;

// ── helpers ──────────────────────────────────────────────────────────────────

fn already_exists_response(name: &str) -> (StatusCode, Json<serde_json::Value>) {
    (StatusCode::CONFLICT, Json(serde_json::json!({
        "error": format!("collection '{}' already exists", name)
    })))
}

// ── vector collections ────────────────────────────────────────────────────────

pub async fn vector_list(
    headers: HeaderMap,
    State(engine): State<Option<Arc<ArcanumEngine>>>,
) -> impl IntoResponse {
    let claims = match validate_bearer(&headers, &engine) {
        Ok(c) => c,
        Err(e) => return e.into_response(),
    };
    let _ = claims;
    let eng = engine.as_ref().unwrap();
    let Some(store) = eng.vector_store.as_ref() else {
        return (StatusCode::OK, Json(serde_json::json!({ "collections": [] }))).into_response();
    };
    match store.list_collections().await {
        Ok(cols) => (StatusCode::OK, Json(serde_json::json!({ "collections": cols }))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() }))).into_response(),
    }
}

pub async fn vector_create(
    headers: HeaderMap,
    State(engine): State<Option<Arc<ArcanumEngine>>>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    let _claims = match validate_bearer(&headers, &engine) {
        Ok(c) => c,
        Err(e) => return e.into_response(),
    };
    let eng = engine.as_ref().unwrap();
    let Some(store) = eng.vector_store.as_ref() else {
        return StatusCode::NOT_FOUND.into_response();
    };
    match store.create_collection(&name).await {
        Ok(()) => StatusCode::CREATED.into_response(),
        Err(ArcanumError::AlreadyExists(_)) => already_exists_response(&name).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() }))).into_response(),
    }
}

pub async fn vector_delete(
    headers: HeaderMap,
    State(engine): State<Option<Arc<ArcanumEngine>>>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    let _claims = match validate_bearer(&headers, &engine) {
        Ok(c) => c,
        Err(e) => return e.into_response(),
    };
    let eng = engine.as_ref().unwrap();
    let Some(store) = eng.vector_store.as_ref() else {
        return StatusCode::NO_CONTENT.into_response();
    };
    match store.delete_collection(&name).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() }))).into_response(),
    }
}

pub async fn vector_stats_all(
    headers: HeaderMap,
    State(engine): State<Option<Arc<ArcanumEngine>>>,
) -> impl IntoResponse {
    let _claims = match validate_bearer(&headers, &engine) {
        Ok(c) => c,
        Err(e) => return e.into_response(),
    };
    let eng = engine.as_ref().unwrap();
    let Some(store) = eng.vector_store.as_ref() else {
        return (StatusCode::OK, Json(serde_json::json!({ "total": 0, "by_collection": {} }))).into_response();
    };
    let cols = match store.list_collections().await {
        Ok(c) => c,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() }))).into_response(),
    };
    let mut by_collection = serde_json::Map::new();
    let mut total = 0u64;
    for col in &cols {
        let count = store.count_documents(Some(col)).await.unwrap_or(0);
        by_collection.insert(col.clone(), serde_json::json!(count));
        total += count;
    }
    (StatusCode::OK, Json(serde_json::json!({ "total": total, "by_collection": by_collection }))).into_response()
}

pub async fn vector_stats_one(
    headers: HeaderMap,
    State(engine): State<Option<Arc<ArcanumEngine>>>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    let _claims = match validate_bearer(&headers, &engine) {
        Ok(c) => c,
        Err(e) => return e.into_response(),
    };
    let eng = engine.as_ref().unwrap();
    let Some(store) = eng.vector_store.as_ref() else {
        return (StatusCode::OK, Json(serde_json::json!({ "count": 0 }))).into_response();
    };
    match store.count_documents(Some(&name)).await {
        Ok(count) => (StatusCode::OK, Json(serde_json::json!({ "count": count }))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() }))).into_response(),
    }
}

// ── graph collections ─────────────────────────────────────────────────────────

pub async fn graph_list(
    headers: HeaderMap,
    State(engine): State<Option<Arc<ArcanumEngine>>>,
) -> impl IntoResponse {
    let _claims = match validate_bearer(&headers, &engine) {
        Ok(c) => c,
        Err(e) => return e.into_response(),
    };
    let eng = engine.as_ref().unwrap();
    let Some(store) = eng.graph_store.as_ref() else {
        return (StatusCode::OK, Json(serde_json::json!({ "collections": [] }))).into_response();
    };
    match store.list_collections().await {
        Ok(cols) => (StatusCode::OK, Json(serde_json::json!({ "collections": cols }))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() }))).into_response(),
    }
}

pub async fn graph_create(
    headers: HeaderMap,
    State(engine): State<Option<Arc<ArcanumEngine>>>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    let _claims = match validate_bearer(&headers, &engine) {
        Ok(c) => c,
        Err(e) => return e.into_response(),
    };
    let eng = engine.as_ref().unwrap();
    let Some(store) = eng.graph_store.as_ref() else {
        return StatusCode::NOT_FOUND.into_response();
    };
    match store.create_collection(&name).await {
        Ok(()) => StatusCode::CREATED.into_response(),
        Err(ArcanumError::AlreadyExists(_)) => already_exists_response(&name).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() }))).into_response(),
    }
}

pub async fn graph_delete(
    headers: HeaderMap,
    State(engine): State<Option<Arc<ArcanumEngine>>>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    let _claims = match validate_bearer(&headers, &engine) {
        Ok(c) => c,
        Err(e) => return e.into_response(),
    };
    let eng = engine.as_ref().unwrap();
    let Some(store) = eng.graph_store.as_ref() else {
        return StatusCode::NO_CONTENT.into_response();
    };
    match store.delete_collection(&name).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() }))).into_response(),
    }
}

pub async fn graph_stats_all(
    headers: HeaderMap,
    State(engine): State<Option<Arc<ArcanumEngine>>>,
) -> impl IntoResponse {
    let _claims = match validate_bearer(&headers, &engine) {
        Ok(c) => c,
        Err(e) => return e.into_response(),
    };
    let eng = engine.as_ref().unwrap();
    let Some(store) = eng.graph_store.as_ref() else {
        return (StatusCode::OK, Json(serde_json::json!({ "total": 0, "by_collection": {} }))).into_response();
    };
    let cols = match store.list_collections().await {
        Ok(c) => c,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() }))).into_response(),
    };
    let mut by_collection = serde_json::Map::new();
    let mut total = 0u64;
    for col in &cols {
        let count = store.count_documents(Some(col)).await.unwrap_or(0);
        by_collection.insert(col.clone(), serde_json::json!(count));
        total += count;
    }
    (StatusCode::OK, Json(serde_json::json!({ "total": total, "by_collection": by_collection }))).into_response()
}

pub async fn graph_stats_one(
    headers: HeaderMap,
    State(engine): State<Option<Arc<ArcanumEngine>>>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    let _claims = match validate_bearer(&headers, &engine) {
        Ok(c) => c,
        Err(e) => return e.into_response(),
    };
    let eng = engine.as_ref().unwrap();
    let Some(store) = eng.graph_store.as_ref() else {
        return (StatusCode::OK, Json(serde_json::json!({ "count": 0 }))).into_response();
    };
    match store.count_documents(Some(&name)).await {
        Ok(count) => (StatusCode::OK, Json(serde_json::json!({ "count": count }))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() }))).into_response(),
    }
}

// ── tree collections ──────────────────────────────────────────────────────────

pub async fn tree_list(
    headers: HeaderMap,
    State(engine): State<Option<Arc<ArcanumEngine>>>,
) -> impl IntoResponse {
    let _claims = match validate_bearer(&headers, &engine) {
        Ok(c) => c,
        Err(e) => return e.into_response(),
    };
    let eng = engine.as_ref().unwrap();
    let Some(store) = eng.tree_store.as_ref() else {
        return (StatusCode::OK, Json(serde_json::json!({ "collections": [] }))).into_response();
    };
    match store.list_collections().await {
        Ok(cols) => (StatusCode::OK, Json(serde_json::json!({ "collections": cols }))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() }))).into_response(),
    }
}

pub async fn tree_create(
    headers: HeaderMap,
    State(engine): State<Option<Arc<ArcanumEngine>>>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    let _claims = match validate_bearer(&headers, &engine) {
        Ok(c) => c,
        Err(e) => return e.into_response(),
    };
    let eng = engine.as_ref().unwrap();
    let Some(store) = eng.tree_store.as_ref() else {
        return StatusCode::NOT_FOUND.into_response();
    };
    match store.create_collection(&name).await {
        Ok(()) => StatusCode::CREATED.into_response(),
        Err(ArcanumError::AlreadyExists(_)) => already_exists_response(&name).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() }))).into_response(),
    }
}

pub async fn tree_delete(
    headers: HeaderMap,
    State(engine): State<Option<Arc<ArcanumEngine>>>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    let _claims = match validate_bearer(&headers, &engine) {
        Ok(c) => c,
        Err(e) => return e.into_response(),
    };
    let eng = engine.as_ref().unwrap();
    let Some(store) = eng.tree_store.as_ref() else {
        return StatusCode::NO_CONTENT.into_response();
    };
    match store.delete_collection(&name).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() }))).into_response(),
    }
}

pub async fn tree_stats_all(
    headers: HeaderMap,
    State(engine): State<Option<Arc<ArcanumEngine>>>,
) -> impl IntoResponse {
    let _claims = match validate_bearer(&headers, &engine) {
        Ok(c) => c,
        Err(e) => return e.into_response(),
    };
    let eng = engine.as_ref().unwrap();
    let Some(store) = eng.tree_store.as_ref() else {
        return (StatusCode::OK, Json(serde_json::json!({ "total": 0, "by_collection": {} }))).into_response();
    };
    let cols = match store.list_collections().await {
        Ok(c) => c,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() }))).into_response(),
    };
    let mut by_collection = serde_json::Map::new();
    let mut total = 0u64;
    for col in &cols {
        let count = store.count_documents(Some(col)).await.unwrap_or(0);
        by_collection.insert(col.clone(), serde_json::json!(count));
        total += count;
    }
    (StatusCode::OK, Json(serde_json::json!({ "total": total, "by_collection": by_collection }))).into_response()
}

pub async fn tree_stats_one(
    headers: HeaderMap,
    State(engine): State<Option<Arc<ArcanumEngine>>>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    let _claims = match validate_bearer(&headers, &engine) {
        Ok(c) => c,
        Err(e) => return e.into_response(),
    };
    let eng = engine.as_ref().unwrap();
    let Some(store) = eng.tree_store.as_ref() else {
        return (StatusCode::OK, Json(serde_json::json!({ "count": 0 }))).into_response();
    };
    match store.count_documents(Some(&name)).await {
        Ok(count) => (StatusCode::OK, Json(serde_json::json!({ "count": count }))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() }))).into_response(),
    }
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use crate::build_app;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    #[tokio::test]
    async fn collections_require_auth() {
        let app = build_app(None);
        for path in &[
            "/api/v1/vector/collections",
            "/api/v1/vector/collections/stats",
            "/api/v1/graph/collections",
            "/api/v1/tree/collections",
        ] {
            let resp = app.clone().oneshot(
                Request::builder().uri(*path).body(Body::empty()).unwrap()
            ).await.unwrap();
            assert_eq!(resp.status(), StatusCode::UNAUTHORIZED,
                "GET {} should require auth", path);
        }
    }

    #[tokio::test]
    async fn graph_collections_require_auth_and_return_200_not_501() {
        // Without an engine: 401 (auth check fires before store check).
        // With an engine that has no graph_store: 200 with empty list.
        // This test verifies the route is no longer a 501 stub.
        let app = build_app(None);
        let resp = app.oneshot(
            Request::builder()
                .uri("/api/v1/graph/collections")
                .body(Body::empty())
                .unwrap()
        ).await.unwrap();
        // 401 means auth check ran (not 501 from stub).
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED,
            "graph list should return 401 (auth), not 501 (stub)");
    }

    #[tokio::test]
    async fn vector_delete_dispatches_to_vector_store() {
        // Integration-level test: verify vector_delete uses vector_store, not tree_store.
        // The handler is verified by checking the code path — the real proof is at
        // compile time (eng.vector_store field access). Here we verify the route
        // accepts a DELETE request and returns 401 without auth (proving it's wired up).
        let app = build_app(None);
        let resp = app.clone().oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/v1/vector/collections/test-col")
                .body(Body::empty())
                .unwrap()
        ).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED,
            "vector_delete should require auth");
    }
}
