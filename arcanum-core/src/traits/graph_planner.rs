use crate::Result;
use async_trait::async_trait;

/// Plans graph traversals from a natural-language query.
/// Returns seed entity names extracted from the query text.
#[async_trait]
pub trait GraphPlanner: Send + Sync {
    async fn plan_entities(&self, query: &str) -> Result<Vec<String>>;
}
