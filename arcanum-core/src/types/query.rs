use serde::{Deserialize, Serialize};
use super::document::CollectionId;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Query {
    pub text: String,
    pub collection_id: Option<CollectionId>,
    pub top_k: usize,
    pub filters: Vec<MetadataFilter>,
}

impl Query {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            collection_id: None,
            top_k: 5,
            filters: vec![],
        }
    }

    pub fn with_collection(mut self, c: CollectionId) -> Self {
        self.collection_id = Some(c);
        self
    }

    pub fn with_top_k(mut self, k: usize) -> Self {
        self.top_k = k;
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetadataFilter {
    pub field: String,
    pub op: FilterOp,
    pub value: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FilterOp {
    Eq,
    Ne,
    Gt,
    Lt,
    In,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_query_builder() {
        let q = Query::new("what is RAG?")
            .with_collection(CollectionId("docs".to_string()))
            .with_top_k(10);
        assert_eq!(q.text, "what is RAG?");
        assert_eq!(q.top_k, 10);
    }

    #[test]
    fn test_metadata_filter() {
        let f = MetadataFilter {
            field: "lang".to_string(),
            op: FilterOp::Eq,
            value: serde_json::json!("en"),
        };
        assert_eq!(f.field, "lang");
    }
}
