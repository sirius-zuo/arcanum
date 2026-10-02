use crate::{traits::store::GraphStore, types::ChunkId, Result};
use async_trait::async_trait;

#[async_trait]
pub trait GraphScorer: Send + Sync {
    /// Score graph chunks reachable from the seed entities, sorted descending.
    async fn score(
        &self,
        graph: &dyn GraphStore,
        collection: &str,
        seeds: &[String],
        limit: usize,
    ) -> Result<Vec<(ChunkId, f32)>>;
}
