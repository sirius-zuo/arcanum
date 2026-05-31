use arcanum_core::Result;
use arcanum_eval::{EvalReport, BenchmarkDataset};

#[derive(Debug)]
pub struct EvalService {}

impl EvalService {
    pub fn new() -> Self {
        Self {}
    }

    pub async fn list_datasets(&self) -> Result<Vec<BenchmarkDataset>> {
        Ok(vec![])
    }

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
