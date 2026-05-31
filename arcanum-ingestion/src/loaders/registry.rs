use arcanum_core::{traits::{DocumentLoader, Source}, types::RawDocument, Result, ArcanumError};
use std::sync::Arc;

pub struct LoaderRegistry {
    loaders: Vec<Arc<dyn DocumentLoader>>,
}

impl LoaderRegistry {
    pub fn new() -> Self { Self { loaders: vec![] } }

    pub fn register(mut self, loader: Arc<dyn DocumentLoader>) -> Self {
        self.loaders.push(loader);
        self
    }

    pub async fn load(&self, source: &Source) -> Result<RawDocument> {
        self.loaders.iter()
            .find(|l| l.supports(source))
            .ok_or_else(|| ArcanumError::Ingestion(
                format!("no loader registered for source: {}", source.display_uri())
            ))?
            .load(source).await
    }
}
