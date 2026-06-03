use arcanum_core::{traits::{DocumentLoader, Source}, types::RawDocument, Result, ArcanumError};
use async_trait::async_trait;
use tracing::instrument;

pub struct GitLoader;
impl GitLoader { pub fn new() -> Self { Self } }

#[async_trait]
impl DocumentLoader for GitLoader {
    #[instrument(skip(self), fields(source_uri = %source.uri(), loader = "git"), err)]
    async fn load(&self, source: &Source) -> Result<RawDocument> {
        let _ = source;
        Err(ArcanumError::Ingestion("GitLoader not yet implemented".into()))
    }
    fn supports(&self, s: &Source) -> bool { matches!(s, Source::Git { .. }) }
}
