use arcanum_core::types::ChunkId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkSample {
    pub query: String,
    pub relevant_chunk_ids: Vec<ChunkId>,
    pub expected_answer: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkDataset {
    pub collection_id: String,
    pub version: String,
    pub samples: Vec<BenchmarkSample>,
}

impl BenchmarkDataset {
    pub fn new(collection_id: &str, version: &str) -> Self {
        Self {
            collection_id: collection_id.to_string(),
            version: version.to_string(),
            samples: Vec::new(),
        }
    }

    pub fn add_sample(&mut self, sample: BenchmarkSample) {
        self.samples.push(sample);
    }

    pub fn from_json(json: &str) -> serde_json::Result<Self> {
        serde_json::from_str(json)
    }

    pub fn to_json(&self) -> serde_json::Result<String> {
        serde_json::to_string_pretty(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn test_benchmark_dataset_add_and_retrieve() {
        let mut dataset = BenchmarkDataset::new("test-collection", "v1");
        dataset.add_sample(BenchmarkSample {
            query: "What is Rust?".to_string(),
            relevant_chunk_ids: vec![ChunkId(Uuid::new_v4())],
            expected_answer: Some("Rust is a systems programming language.".to_string()),
        });
        assert_eq!(dataset.samples.len(), 1);
        assert_eq!(dataset.collection_id, "test-collection");
        assert_eq!(dataset.version, "v1");
    }

    #[test]
    fn test_benchmark_dataset_serializes_to_json() {
        let mut dataset = BenchmarkDataset::new("col", "v2");
        dataset.add_sample(BenchmarkSample {
            query: "test".to_string(),
            relevant_chunk_ids: vec![],
            expected_answer: None,
        });
        let json = serde_json::to_string(&dataset).unwrap();
        assert!(json.contains("col"));
        assert!(json.contains("v2"));
    }
}
