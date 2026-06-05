use axum::{extract::{Query, State}, http::{HeaderMap, StatusCode}, response::IntoResponse, Json};
use serde::Serialize;
use std::sync::Arc;
use arcanum_core::traits::GraphQuery;
use arcanum_engine::ArcanumEngine;
use crate::routes::auth::validate_bearer;

#[derive(serde::Deserialize)]
pub struct GraphQueryParams {
    pub collection_id: Option<String>,
}

#[derive(Serialize)]
pub struct GraphNode {
    pub id: String,
    pub name: String,
    pub entity_type: String,
}

#[derive(Serialize)]
pub struct GraphEdge {
    pub source: String,
    pub target: String,
    pub label: String,
}

#[derive(Serialize)]
pub struct GraphView {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
}

/// GET /api/v1/graph?collection_id=X — returns the knowledge graph for a collection.
pub async fn get_graph(
    headers: HeaderMap,
    State(engine): State<Option<Arc<ArcanumEngine>>>,
    query: axum::extract::Query<GraphQueryParams>,
) -> impl IntoResponse {
    if let Err(e) = validate_bearer(&headers, &engine) {
        return e.into_response();
    }
    let eng = engine.as_ref().unwrap();
    let collection = query.collection_id.as_deref().unwrap_or("");

    let Some(store) = eng.graph_store.as_ref() else {
        return (StatusCode::OK, Json(GraphView { nodes: vec![], edges: vec![] })).into_response();
    };

    // All entities: a query with no name/type filter matches everything.
    let q = GraphQuery { entity_name: None, entity_type: None, max_hops: 1, relation_filter: None };
    let entities = store.query(collection, &q).await.unwrap_or_default();

    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    for e in &entities {
        nodes.push(GraphNode {
            id: e.id.0.to_string(),
            name: e.name.clone(),
            entity_type: e.entity_type.clone(),
        });
        if let Ok(relations) = store.get_relations(&e.id).await {
            for r in relations {
                edges.push(GraphEdge {
                    source: r.source.0.to_string(),
                    target: r.target.0.to_string(),
                    label: r.relation_type.clone(),
                });
            }
        }
    }

    (StatusCode::OK, Json(GraphView { nodes, edges })).into_response()
}

#[cfg(test)]
mod tests {
    use crate::build_app;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    #[tokio::test]
    async fn graph_requires_auth() {
        // No engine (test mode) → fail-closed 401.
        let app = build_app(None);
        let resp = app.oneshot(
            Request::builder().uri("/api/v1/graph").body(Body::empty()).unwrap()
        ).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }
}
