use arcanum_core::Result;
use arcanum_eval::{EvalReport, BenchmarkDataset};
use tracing::instrument;

#[derive(Debug)]
pub struct EvalService {}

impl EvalService {
    pub fn new() -> Self {
        Self {}
    }

    #[instrument(skip(self), err)]
    pub async fn list_datasets(&self) -> Result<Vec<BenchmarkDataset>> {
        Ok(vec![])
    }

    #[instrument(skip(self), fields(dataset_id = _dataset_id), err)]
    pub async fn get_report(&self, _dataset_id: &str) -> Result<Option<EvalReport>> {
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_eval_service_list_datasets_empty() {
        let svc = EvalService::new();
        let datasets = svc.list_datasets().await.unwrap();
        assert!(datasets.is_empty());
    }

    #[tokio::test]
    async fn test_eval_service_get_report_none() {
        let svc = EvalService::new();
        let report = svc.get_report("nonexistent").await.unwrap();
        assert!(report.is_none());
    }
}
