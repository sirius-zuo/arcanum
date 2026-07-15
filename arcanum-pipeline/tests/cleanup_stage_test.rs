use arcanum_pipeline::{
    dag::{StageContext, CTX_REPLACE},
    executor::DagExecutor,
    ingestion_state::IngestionState,
    stages::{make_load_stage, make_dedup_stage, make_cleanup_stage},
    PipelineDAG,
};
use arcanum_core::{
    traits::{DocumentVersionStore, VectorStore, Source, VectorQuery, ScoredChunk},
    types::{CollectionId, DocumentId, DocumentEntry, DocumentVersion, VersioningPolicy, IndexedChunk, ChunkId},
    Result,
};
use arcanum_ingestion::{LoaderRegistry, RawLoader};
use async_trait::async_trait;
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
use tokio::sync::Mutex;

fn make_state(content: Vec<u8>, uri: &str) -> Arc<Mutex<IngestionState>> {
    Arc::new(Mutex::new(IngestionState::new(
        Source::Raw {
            content,
            mime_hint: Some("text/plain".into()),
            uri: uri.into(),
        },
        CollectionId("col".into()),
    )))
}

fn make_loaders() -> Arc<LoaderRegistry> {
    Arc::new(LoaderRegistry::new().register(Arc::new(RawLoader::new())))
}

/// Panics if supersede_active is ever called — cleanup must never call it,
/// since state.snapshot_document_id (the field its guard reads) is only
/// ever set by make_snapshot_stage, which runs strictly after cleanup in
/// every template's DAG. Real supersede happens from make_snapshot_stage.
struct PanicsOnSupersedeVersionStore;

#[async_trait]
impl DocumentVersionStore for PanicsOnSupersedeVersionStore {
    async fn get_latest(&self, _: &str, _: &str) -> Result<Option<DocumentVersion>> { Ok(None) }
    async fn get_versioning_policy(&self, _: &str) -> Result<VersioningPolicy> { Ok(VersioningPolicy::Replace) }
    async fn add_version(&self, _: DocumentVersion) -> Result<()> { Ok(()) }
    async fn supersede_active(&self, _: &DocumentId) -> Result<()> {
        panic!("cleanup stage must never call supersede_active — state.snapshot_document_id \
                is only set by make_snapshot_stage, which runs after cleanup");
    }
    async fn list_versions(&self, _: &DocumentId) -> Result<Vec<DocumentVersion>> { Ok(vec![]) }
    async fn set_versioning_policy(&self, _: &str, _: VersioningPolicy) -> Result<()> { Ok(()) }
    async fn delete_by_source_uri(&self, _: &str, _: &str) -> Result<()> { Ok(()) }
    async fn get_version(&self, _: &DocumentId, _: u32) -> Result<Option<DocumentVersion>> { Ok(None) }
    async fn list_collections(&self) -> Result<Vec<String>> { Ok(vec![]) }
    async fn list_documents(&self, _: &str) -> Result<Vec<DocumentEntry>> { Ok(vec![]) }
}

struct RecordingVectorStore(Arc<AtomicBool>);

#[async_trait]
impl VectorStore for RecordingVectorStore {
    async fn upsert(&self, _: &str, _: Vec<IndexedChunk>) -> Result<()> { Ok(()) }
    async fn search(&self, _: &str, _: &VectorQuery) -> Result<Vec<ScoredChunk>> { Ok(vec![]) }
    async fn delete(&self, _: &str, _: &[ChunkId]) -> Result<()> { Ok(()) }
    async fn collection_exists(&self, _: &str) -> Result<bool> { Ok(true) }
    async fn delete_by_source_uri(&self, _: &str, _: &str) -> Result<()> {
        self.0.store(true, Ordering::SeqCst);
        Ok(())
    }
}

#[tokio::test]
async fn cleanup_stage_never_calls_supersede_active() {
    let state = make_state(b"replacing content".to_vec(), "file://cleanup-test.txt");
    let deleted = Arc::new(AtomicBool::new(false));
    let vector_store = Arc::new(RecordingVectorStore(deleted.clone()));

    let dag = PipelineDAG::new()
        .add_stage(make_load_stage(state.clone(), make_loaders()))
        .add_stage(make_dedup_stage(state.clone(), Arc::new(PanicsOnSupersedeVersionStore)))
        .add_stage(make_cleanup_stage(
            state.clone(),
            Arc::new(PanicsOnSupersedeVersionStore),
            vector_store,
            None,
            None,
        ));

    let mut ctx = StageContext::default();
    ctx.insert(CTX_REPLACE.to_string(), serde_json::json!(true));
    DagExecutor::execute(&dag, ctx).await.unwrap();

    assert!(
        deleted.load(Ordering::SeqCst),
        "cleanup stage should still delete stale store data on replace"
    );
    // If supersede_active had been called, PanicsOnSupersedeVersionStore
    // would have panicked and this test would already have failed above.
}
