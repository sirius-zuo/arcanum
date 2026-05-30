use arcanum_core::{traits::{DocumentLoader, Source}, types::RawDocument, Result, ArcanumError};
use async_trait::async_trait;

pub struct GitLoader;
impl GitLoader { pub fn new() -> Self { Self } }

#[async_trait]
impl DocumentLoader for GitLoader {
    async fn load(&self, _: &Source) -> Result<RawDocument> {
        Err(ArcanumError::Ingestion("GitLoader not yet implemented".into()))
    }
    fn supports(&self, s: &Source) -> bool { matches!(s, Source::Git { .. }) }
}
