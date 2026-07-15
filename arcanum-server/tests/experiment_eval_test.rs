use arcanum_core::traits::{VectorStore, VectorQuery, ScoredChunk};
use arcanum_core::types::*;
use arcanum_engine::ArcanumEngine;
use async_trait::async_trait;
use axum::body::Body;
use axum::http::{Request, StatusCode, Method};
use std::sync::Arc;
use tower::ServiceExt;

/// Returns different chunks depending on which collection/namespace is
/// searched, so champion vs challenger recall are distinguishably different
/// — proving the eval route actually searched both namespaces, not just one
/// twice.
struct NamespaceAwareVectorStore;
#[async_trait]
impl VectorStore for NamespaceAwareVectorStore {
    async fn upsert(&self, _: &str, _: Vec<IndexedChunk>) -> arcanum_core::Result<()> { Ok(()) }
    async fn search(&self, collection: &str, _q: &VectorQuery) -> arcanum_core::Result<Vec<ScoredChunk>> {
        // Only the shadow namespace returns the chunk id the test marks as
        // relevant; the live namespace returns a different id, so recall@5
        // differs between champion and challenger.
        let (id, text) = if collection.contains("__shadow_") {
            (ChunkId(uuid::Uuid::nil()), "relevant hit")
        } else {
            (ChunkId(uuid::Uuid::from_u128(1)), "irrelevant miss")
        };
        Ok(vec![ScoredChunk {
            chunk: IndexedChunk {
                chunk: Chunk {
                    id,
                    text: text.into(),
                    document_id: DocumentId::new(),
                    collection_id: CollectionId(collection.to_string()),
                    position: ChunkPosition { start: 0, end: 1, index: 0 },
                    metadata: ChunkMetadata::default(),
                    provenance: ChunkProvenance::default(),
                },
                vector: Vector(vec![0.1, 0.2]),
                token_vectors: None,
                store_id: String::new(),
            },
            score: 0.9,
        }])
    }
    async fn delete(&self, _: &str, _: &[ChunkId]) -> arcanum_core::Result<()> { Ok(()) }
    async fn collection_exists(&self, _: &str) -> arcanum_core::Result<bool> { Ok(true) }
    async fn delete_by_source_uri(&self, _: &str, _: &str) -> arcanum_core::Result<()> { Ok(()) }
}

struct FakeEmbedder;
#[async_trait]
impl arcanum_core::traits::Embedder for FakeEmbedder {
    async fn embed(&self, texts: Vec<String>) -> arcanum_core::Result<Vec<Vector>> {
        Ok(texts.iter().map(|_| Vector(vec![0.1, 0.2])).collect())
    }
    fn dimension(&self) -> usize { 2 }
}

#[tokio::test]
async fn eval_route_computes_distinct_champion_and_challenger_recall() {
    let engine = ArcanumEngine::builder()
        .auth_secret("a-32-char-secret-for-testing-ok!")
        .vector_store(Arc::new(NamespaceAwareVectorStore))
        .embedder(Arc::new(FakeEmbedder))
        .version_store(Arc::new(arcanum_core::traits::NoOpDocumentVersionStore))
        .build().await
        .expect("build should succeed");

    let token = engine.auth.generate_admin_key("tester");
    let claims = engine.auth.validate_api_key(&token).unwrap();
    engine.collection.create(CollectionId("col1".into()), "test collection".into(), &claims)
        .await
        .expect("collection create should succeed");

    let exp = engine.experiment
        .start(CollectionId("col1".into()), PerBackendChunkConfig::default())
        .await
        .expect("start should succeed");

    let relevant_id = ChunkId(uuid::Uuid::nil());
    let body = serde_json::json!([
        { "query": "hello", "relevant_chunk_ids": [relevant_id.0.to_string()] }
    ]);

    let app = arcanum_server::build_app(Some(engine.clone()));
    let resp = app.oneshot(
        Request::builder()
            .method(Method::POST)
            .uri(format!("/api/v1/collections/col1/experiments/{}/eval", exp.id.0))
            .header("Authorization", format!("Bearer {token}"))
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap()
    ).await.unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = http_body_util::BodyExt::collect(resp.into_body()).await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();

    let champion = json["metrics"]["champion_recall_at_5"].as_f64().unwrap();
    let challenger = json["metrics"]["challenger_recall_at_5"].as_f64().unwrap();
    assert!(challenger > champion, "shadow namespace returned the relevant chunk, live namespace didn't: champion={champion} challenger={challenger}");

    // The real ExperimentService state should reflect the computed metrics.
    let updated = engine.experiment.get("col1", &exp.id).await.unwrap();
    assert!(updated.metrics.is_some(), "update_metrics should have been called");
}
