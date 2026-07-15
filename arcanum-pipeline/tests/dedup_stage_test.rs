use arcanum_pipeline::{
    dag::{StageContext, CTX_FORCE, CTX_REPLACE, CTX_SKIP},
    executor::DagExecutor,
    ingestion_state::IngestionState,
    stages::{make_load_stage, make_dedup_stage},
    PipelineDAG,
};
use arcanum_core::{
    traits::{DocumentVersionStore, NoOpDocumentVersionStore, Source},
    types::{CollectionId, DocumentEntry, DocumentId, DocumentVersion, VersionStatus, VersioningPolicy, RawDocument},
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

struct MatchingHashStore { hash: String }

#[async_trait]
impl DocumentVersionStore for MatchingHashStore {
    async fn get_latest(&self, _: &str, _: &str) -> Result<Option<DocumentVersion>> {
        Ok(Some(DocumentVersion {
            document_id: DocumentId::new(),
            version_num: 1,
            source_uri: "file://test.txt".into(),
            collection_id: "col".into(),
            content_hash: self.hash.clone(),
            snapshot_uri: "mem://test".into(),
            canonical_uri: None,
            mime_type: "text/plain".into(),
            status: VersionStatus::Active,
            ingested_at: chrono::Utc::now(),
            extra: std::collections::HashMap::new(),
        }))
    }
    async fn get_versioning_policy(&self, _: &str) -> Result<VersioningPolicy> {
        Ok(VersioningPolicy::AppendOnly)
    }
    async fn add_version(&self, _: DocumentVersion) -> Result<()> { Ok(()) }
    async fn supersede_active(&self, _: &DocumentId) -> Result<()> { Ok(()) }
    async fn list_versions(&self, _: &DocumentId) -> Result<Vec<DocumentVersion>> { Ok(vec![]) }
    async fn set_versioning_policy(&self, _: &str, _: VersioningPolicy) -> Result<()> { Ok(()) }
    async fn delete_by_source_uri(&self, _: &str, _: &str) -> Result<()> { Ok(()) }
    async fn get_version(&self, _: &DocumentId, _: u32) -> Result<Option<DocumentVersion>> { Ok(None) }
    async fn list_collections(&self) -> Result<Vec<String>> { Ok(vec![]) }
    async fn list_documents(&self, _: &str) -> Result<Vec<DocumentEntry>> { Ok(vec![]) }
}

struct DifferentHashStore;

#[async_trait]
impl DocumentVersionStore for DifferentHashStore {
    async fn get_latest(&self, _: &str, _: &str) -> Result<Option<DocumentVersion>> {
        Ok(Some(DocumentVersion {
            document_id: DocumentId::new(),
            version_num: 1,
            source_uri: "file://test.txt".into(),
            collection_id: "col".into(),
            content_hash: "old_hash_xyz".into(),
            snapshot_uri: "mem://test".into(),
            canonical_uri: None,
            mime_type: "text/plain".into(),
            status: VersionStatus::Active,
            ingested_at: chrono::Utc::now(),
            extra: std::collections::HashMap::new(),
        }))
    }
    async fn get_versioning_policy(&self, _: &str) -> Result<VersioningPolicy> {
        Ok(VersioningPolicy::AppendOnly)
    }
    async fn add_version(&self, _: DocumentVersion) -> Result<()> { Ok(()) }
    async fn supersede_active(&self, _: &DocumentId) -> Result<()> { Ok(()) }
    async fn list_versions(&self, _: &DocumentId) -> Result<Vec<DocumentVersion>> { Ok(vec![]) }
    async fn set_versioning_policy(&self, _: &str, _: VersioningPolicy) -> Result<()> { Ok(()) }
    async fn delete_by_source_uri(&self, _: &str, _: &str) -> Result<()> { Ok(()) }
    async fn get_version(&self, _: &DocumentId, _: u32) -> Result<Option<DocumentVersion>> { Ok(None) }
    async fn list_collections(&self) -> Result<Vec<String>> { Ok(vec![]) }
    async fn list_documents(&self, _: &str) -> Result<Vec<DocumentEntry>> { Ok(vec![]) }
}

struct NoVersionStore;

#[async_trait]
impl DocumentVersionStore for NoVersionStore {
    async fn get_latest(&self, _: &str, _: &str) -> Result<Option<DocumentVersion>> {
        Ok(None)
    }
    async fn get_versioning_policy(&self, _: &str) -> Result<VersioningPolicy> {
        Ok(VersioningPolicy::Replace)
    }
    async fn add_version(&self, _: DocumentVersion) -> Result<()> { Ok(()) }
    async fn supersede_active(&self, _: &DocumentId) -> Result<()> { Ok(()) }
    async fn list_versions(&self, _: &DocumentId) -> Result<Vec<DocumentVersion>> { Ok(vec![]) }
    async fn set_versioning_policy(&self, _: &str, _: VersioningPolicy) -> Result<()> { Ok(()) }
    async fn delete_by_source_uri(&self, _: &str, _: &str) -> Result<()> { Ok(()) }
    async fn get_version(&self, _: &DocumentId, _: u32) -> Result<Option<DocumentVersion>> { Ok(None) }
    async fn list_collections(&self) -> Result<Vec<String>> { Ok(vec![]) }
    async fn list_documents(&self, _: &str) -> Result<Vec<DocumentEntry>> { Ok(vec![]) }
}

#[tokio::test]
async fn dedup_stage_skips_on_identical_content() {
    let content = b"hello world".to_vec();
    let doc = RawDocument {
        id: DocumentId::new(),
        content: content.clone(),
        mime_type: "text/plain".into(),
        source_uri: "file://test.txt".into(),
        metadata: Default::default(),
    };
    let content_hash = doc.content_hash();

    let state = make_state(content, "file://test.txt");
    let store = Arc::new(MatchingHashStore { hash: content_hash });
    let dag = PipelineDAG::new()
        .add_stage(make_load_stage(state.clone(), make_loaders()))
        .add_stage(make_dedup_stage(state.clone(), store));

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
        .add_stage(make_dedup_stage(state.clone(), Arc::new(DifferentHashStore)));

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
        .add_stage(make_dedup_stage(state.clone(), Arc::new(NoVersionStore)));

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
        .add_stage(make_dedup_stage(state.clone(), Arc::new(NoVersionStore)));

    let ctx = DagExecutor::execute(&dag, initial_ctx).await.unwrap();
    assert_eq!(
        ctx.get(CTX_REPLACE).and_then(|v| v.as_bool()),
        Some(true),
        "__force should set __replace"
    );
}

#[tokio::test]
async fn dedup_stage_new_version_store_is_treated_as_new() {
    let state = make_state(b"any content".to_vec(), "file://noop.txt");
    let dag = PipelineDAG::new()
        .add_stage(make_load_stage(state.clone(), make_loaders()))
        .add_stage(make_dedup_stage(state.clone(), Arc::new(NoOpDocumentVersionStore)));

    let ctx = DagExecutor::execute(&dag, StageContext::default()).await.unwrap();
    assert!(ctx.get(CTX_SKIP).is_none(), "NoOp store should treat as new document");
    assert!(ctx.get(CTX_REPLACE).is_none(), "NoOp store should not trigger replace");
}
