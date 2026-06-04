use arcanum_pipeline::{
    dag::{StageContext, CTX_FORCE, CTX_REPLACE, CTX_SKIP},
    executor::DagExecutor,
    ingestion_state::IngestionState,
    stages::{make_load_stage, make_dedup_stage},
    PipelineDAG,
};
use arcanum_core::{
    traits::{DocumentRegistry, RegistryEntry, RegistryStatus, NoOpDocumentRegistry, Source},
    types::{CollectionId, RawDocument, DocumentId},
    Result,
};
use arcanum_ingestion::{LoaderRegistry, RawLoader};
use async_trait::async_trait;
use std::sync::Arc;
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

struct MatchingHashRegistry { hash: String }

#[async_trait]
impl DocumentRegistry for MatchingHashRegistry {
    async fn get_entry(&self, _: &str, _: &str) -> Result<Option<RegistryEntry>> {
        Ok(Some(RegistryEntry { content_hash: Some(self.hash.clone()), status: RegistryStatus::Clean }))
    }
    async fn set_replacing(&self, _: &str, _: &str) -> Result<()> { Ok(()) }
    async fn register(&self, _: &str, _: &str, _: &str) -> Result<()> { Ok(()) }
    async fn deregister(&self, _: &str, _: &str) -> Result<()> { Ok(()) }
}

struct DifferentHashRegistry;

#[async_trait]
impl DocumentRegistry for DifferentHashRegistry {
    async fn get_entry(&self, _: &str, _: &str) -> Result<Option<RegistryEntry>> {
        Ok(Some(RegistryEntry { content_hash: Some("old_hash_xyz".into()), status: RegistryStatus::Clean }))
    }
    async fn set_replacing(&self, _: &str, _: &str) -> Result<()> { Ok(()) }
    async fn register(&self, _: &str, _: &str, _: &str) -> Result<()> { Ok(()) }
    async fn deregister(&self, _: &str, _: &str) -> Result<()> { Ok(()) }
}

struct ResumingRegistry;

#[async_trait]
impl DocumentRegistry for ResumingRegistry {
    async fn get_entry(&self, _: &str, _: &str) -> Result<Option<RegistryEntry>> {
        Ok(Some(RegistryEntry { content_hash: None, status: RegistryStatus::Replacing }))
    }
    async fn set_replacing(&self, _: &str, _: &str) -> Result<()> { Ok(()) }
    async fn register(&self, _: &str, _: &str, _: &str) -> Result<()> { Ok(()) }
    async fn deregister(&self, _: &str, _: &str) -> Result<()> { Ok(()) }
}

#[tokio::test]
async fn dedup_stage_skips_on_identical_content() {
    let content = b"hello world".to_vec();
    // Compute the hash the same way RawDocument::content_hash() does
    let doc = RawDocument {
        id: DocumentId::new(),
        content: content.clone(),
        mime_type: "text/plain".into(),
        source_uri: "file://test.txt".into(),
        metadata: Default::default(),
    };
    let content_hash = doc.content_hash();

    let state = make_state(content, "file://test.txt");
    let registry = Arc::new(MatchingHashRegistry { hash: content_hash });
    let dag = PipelineDAG::new()
        .add_stage(make_load_stage(state.clone(), make_loaders()))
        .add_stage(make_dedup_stage(state.clone(), registry));

    let ctx = DagExecutor::execute(&dag, StageContext::default()).await.unwrap();
    assert_eq!(
        ctx.get(CTX_SKIP).and_then(|v| v.as_bool()),
        Some(true),
        "dedup stage should set __skip for identical content"
    );
}

#[tokio::test]
async fn dedup_stage_sets_replace_on_changed_content() {
    let state = make_state(b"new content".to_vec(), "file://test.txt");
    let dag = PipelineDAG::new()
        .add_stage(make_load_stage(state.clone(), make_loaders()))
        .add_stage(make_dedup_stage(state.clone(), Arc::new(DifferentHashRegistry)));

    let ctx = DagExecutor::execute(&dag, StageContext::default()).await.unwrap();
    assert_eq!(
        ctx.get(CTX_REPLACE).and_then(|v| v.as_bool()),
        Some(true),
        "dedup stage should set __replace for changed content"
    );
}

#[tokio::test]
async fn dedup_stage_new_document_proceeds_normally() {
    let state = make_state(b"brand new doc".to_vec(), "file://new.txt");
    let dag = PipelineDAG::new()
        .add_stage(make_load_stage(state.clone(), make_loaders()))
        .add_stage(make_dedup_stage(state.clone(), Arc::new(NoOpDocumentRegistry)));

    let ctx = DagExecutor::execute(&dag, StageContext::default()).await.unwrap();
    assert!(ctx.get(CTX_SKIP).is_none(), "new document should not be skipped");
    assert!(ctx.get(CTX_REPLACE).is_none(), "new document should not trigger replace");
}

#[tokio::test]
async fn dedup_stage_force_sets_replace() {
    let state = make_state(b"any content".to_vec(), "file://forced.txt");
    let mut initial_ctx = StageContext::default();
    initial_ctx.insert(CTX_FORCE.to_string(), serde_json::json!(true));

    let dag = PipelineDAG::new()
        .add_stage(make_load_stage(state.clone(), make_loaders()))
        .add_stage(make_dedup_stage(state.clone(), Arc::new(NoOpDocumentRegistry)));

    let ctx = DagExecutor::execute(&dag, initial_ctx).await.unwrap();
    assert_eq!(
        ctx.get(CTX_REPLACE).and_then(|v| v.as_bool()),
        Some(true),
        "__force should set __replace"
    );
}

#[tokio::test]
async fn dedup_stage_resumes_replacing_status() {
    let state = make_state(b"any content".to_vec(), "file://partial.txt");
    let dag = PipelineDAG::new()
        .add_stage(make_load_stage(state.clone(), make_loaders()))
        .add_stage(make_dedup_stage(state.clone(), Arc::new(ResumingRegistry)));

    let ctx = DagExecutor::execute(&dag, StageContext::default()).await.unwrap();
    assert_eq!(
        ctx.get(CTX_REPLACE).and_then(|v| v.as_bool()),
        Some(true),
        "Replacing status should resume with __replace"
    );
}
