use arcanum_core::{
    traits::{Embedder, NoOpDocumentVersionStore, Preprocessor, VectorStore, VectorQuery, ScoredChunk},
    types::*,
};
use arcanum_engine::{ArcanumEngine, IngestRequest};
use async_trait::async_trait;
use std::sync::{Arc, Mutex};
use std::time::Duration;

struct RecordingVectorStore(Mutex<Vec<String>>);

#[async_trait]
impl VectorStore for RecordingVectorStore {
    async fn upsert(&self, collection: &str, _chunks: Vec<IndexedChunk>) -> arcanum_core::Result<()> {
        self.0.lock().unwrap().push(collection.to_string());
        Ok(())
    }
    async fn search(&self, _c: &str, _q: &VectorQuery) -> arcanum_core::Result<Vec<ScoredChunk>> {
        Ok(vec![])
    }
    async fn delete(&self, _: &str, _: &[ChunkId]) -> arcanum_core::Result<()> { Ok(()) }
    async fn collection_exists(&self, _: &str) -> arcanum_core::Result<bool> { Ok(true) }
    async fn delete_by_source_uri(&self, _: &str, _: &str) -> arcanum_core::Result<()> { Ok(()) }
}

struct StubEmbedder;

#[async_trait]
impl Embedder for StubEmbedder {
    async fn embed(&self, texts: Vec<String>) -> arcanum_core::Result<Vec<Vector>> {
        Ok(texts.iter().map(|_| Vector(vec![0.1, 0.2])).collect())
    }
    fn dimension(&self) -> usize { 2 }
}

struct StubPreprocessor;

#[async_trait]
impl Preprocessor for StubPreprocessor {
    async fn process(&self, doc: RawDocument) -> arcanum_core::Result<RawDocument> { Ok(doc) }
}

#[tokio::test]
async fn test_ingest_task_executes_and_writes_to_vector_store() {
    let store = Arc::new(RecordingVectorStore(Mutex::new(vec![])));
    let engine = ArcanumEngine::builder()
        .auth_secret("a-32-char-secret-for-testing-ok!")
        .vector_store(store.clone())
        .embedder(Arc::new(StubEmbedder))
        .version_store(Arc::new(NoOpDocumentVersionStore))
        .register_preprocessor("default", Arc::new(StubPreprocessor))
        .build()
        .await
        .expect("build should succeed");

    let req = IngestRequest {
        source_uri: "raw://hello world document".into(),
        collection_id: CollectionId("test-col".into()),
        pipeline_template: None,
        force: false,
        content: None,
        mime_hint: None,
    };
    let op_id = engine.ingestion.ingest(req, "test-user").await
        .expect("ingest should queue successfully");
    assert!(!op_id.0.is_nil(), "operation id should be valid");

    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    loop {
        if !store.0.lock().unwrap().is_empty() { break; }
        if std::time::Instant::now() > deadline {
            panic!("worker did not execute the task within 2 seconds");
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    let writes = store.0.lock().unwrap();
    assert!(writes.contains(&"test-col".to_string()),
        "vector store should have received upsert for 'test-col'");
}

#[tokio::test]
async fn test_ingest_fails_when_no_preprocessor_configured() {
    // Built via the real public ArcanumEngineBuilder API: no docling config,
    // no register_preprocessor() call. Per spec, the engine must still build
    // successfully — only the ingestion task itself should fail.
    let store = Arc::new(RecordingVectorStore(Mutex::new(vec![])));
    let engine = ArcanumEngine::builder()
        .auth_secret("a-32-char-secret-for-testing-ok!")
        .vector_store(store.clone())
        .embedder(Arc::new(StubEmbedder))
        .version_store(Arc::new(NoOpDocumentVersionStore))
        .build()
        .await
        .expect("build should succeed even with no preprocessor configured at all");

    let req = IngestRequest {
        source_uri: "raw://unprocessed.pdf".into(),
        collection_id: CollectionId("no-preprocessor-col".into()),
        pipeline_template: None,
        force: false,
        content: Some(b"%PDF-1.4 fake pdf bytes, never extracted".to_vec()),
        mime_hint: Some("application/pdf".into()),
    };
    engine.ingestion.ingest(req, "test-user").await
        .expect("ingest should queue successfully — only the worker's preprocess stage should fail");

    // Give the worker ample time to process (and fail) the task, including any retries.
    tokio::time::sleep(Duration::from_secs(1)).await;

    assert!(store.0.lock().unwrap().is_empty(),
        "no preprocessor is configured (no docling, no register_preprocessor) — the ingestion \
         task must fail at the preprocess stage and never reach the vector store; a write here \
         means a preprocessor is silently passing documents through unprocessed");
}

#[tokio::test]
async fn test_search_with_wired_engine_returns_empty_from_stub_store() {
    let engine = ArcanumEngine::builder()
        .auth_secret("a-32-char-secret-for-testing-ok!")
        .vector_store(Arc::new(RecordingVectorStore(Mutex::new(vec![]))))
        .embedder(Arc::new(StubEmbedder))
        .version_store(Arc::new(NoOpDocumentVersionStore))
        .build()
        .await
        .expect("build should succeed");

    let token = engine.auth.generate_api_key("user1", vec!["col1".into()]);
    let claims = engine.auth.validate_api_key(&token).unwrap();
    let query = Query::new("test").with_collection(CollectionId("col1".into()));
    let result = engine.retrieval.search(query, &claims).await
        .expect("search should not error");
    assert!(result.chunks.is_empty());
}
