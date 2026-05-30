use arcanum_core::{traits::{DocumentLoader, Source}, types::RawDocument, Result, ArcanumError};
use async_trait::async_trait;

pub struct CloudStorageLoader;
impl CloudStorageLoader { pub fn new() -> Self { Self } }

#[async_trait]
impl DocumentLoader for CloudStorageLoader {
    async fn load(&self, _: &Source) -> Result<RawDocument> {
        Err(ArcanumError::Ingestion("CloudStorageLoader not yet implemented".into()))
    }
    fn supports(&self, s: &Source) -> bool { matches!(s, Source::CloudStorage { .. }) }
}
