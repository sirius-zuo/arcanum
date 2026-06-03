use arcanum_core::{traits::{DocumentLoader, Source}, types::RawDocument, Result, ArcanumError};
use async_trait::async_trait;
use tracing::instrument;

pub struct DatabaseLoader;
impl DatabaseLoader { pub fn new() -> Self { Self } }

#[async_trait]
impl DocumentLoader for DatabaseLoader {
    #[instrument(skip(self), fields(source_uri = %source.uri(), loader = "database"), err)]
    async fn load(&self, source: &Source) -> Result<RawDocument> {
        let _ = source;
        Err(ArcanumError::Ingestion("DatabaseLoader not yet implemented".into()))
    }
    fn supports(&self, s: &Source) -> bool { matches!(s, Source::Database { .. }) }
}
