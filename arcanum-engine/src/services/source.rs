use arcanum_core::{Result, ArcanumError};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::instrument;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngestionSource {
    pub id: String,
    pub uri: String,
    pub collection_id: String,
    pub pipeline_template: Option<String>,
}

#[derive(Debug)]
pub struct IngestionSourceService {
    sources: Arc<RwLock<Vec<IngestionSource>>>,
}

impl IngestionSourceService {
    pub fn new() -> Self {
        Self { sources: Arc::new(RwLock::new(vec![])) }
    }

    #[instrument(skip(self, source), fields(source_id = %source.id), err)]
    pub async fn create(&self, source: IngestionSource) -> Result<()> {
        let mut sources = self.sources.write().await;
        if sources.iter().any(|s| s.id == source.id) {
            return Err(ArcanumError::Storage(format!("source '{}' already exists", source.id)));
        }
        sources.push(source);
        Ok(())
    }

    #[instrument(skip(self))]
    pub async fn list(&self) -> Vec<IngestionSource> {
        self.sources.read().await.clone()
    }

    #[instrument(skip(self), fields(source_id = id), err)]
    pub async fn delete(&self, id: &str) -> Result<()> {
        let mut sources = self.sources.write().await;
        let before = sources.len();
        sources.retain(|s| s.id != id);
        if sources.len() == before {
            return Err(ArcanumError::NotFound(format!("source '{}'", id)));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_ingestion_source_create_and_list() {
        let svc = IngestionSourceService::new();
        let src = IngestionSource {
            id: "src-1".into(),
            uri: "s3://bucket/key".into(),
            collection_id: "col-1".into(),
            pipeline_template: None,
        };
        svc.create(src.clone()).await.unwrap();
        let list = svc.list().await;
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, "src-1");
    }

    #[tokio::test]
    async fn test_ingestion_source_delete() {
        let svc = IngestionSourceService::new();
        let src = IngestionSource {
            id: "src-2".into(),
            uri: "gs://bucket/key".into(),
            collection_id: "col-2".into(),
            pipeline_template: Some("standard".into()),
        };
        svc.create(src).await.unwrap();
        svc.delete("src-2").await.unwrap();
        assert!(svc.list().await.is_empty());
    }

    #[tokio::test]
    async fn test_ingestion_source_delete_not_found() {
        let svc = IngestionSourceService::new();
        let result = svc.delete("nonexistent").await;
        assert!(result.is_err());
    }
}
