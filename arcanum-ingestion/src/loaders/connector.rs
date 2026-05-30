use arcanum_core::{traits::{DocumentLoader, Source}, types::RawDocument, Result, ArcanumError};
use async_trait::async_trait;

pub struct ConnectorLoader;
impl ConnectorLoader { pub fn new() -> Self { Self } }

#[async_trait]
impl DocumentLoader for ConnectorLoader {
    async fn load(&self, _: &Source) -> Result<RawDocument> {
        Err(ArcanumError::Ingestion("ConnectorLoader not yet implemented".into()))
    }
    fn supports(&self, s: &Source) -> bool { matches!(s, Source::Connector { .. }) }
}
