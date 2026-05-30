use crate::{deps::PipelineDeps, ingestion_state::IngestionState, registry::TemplateBuilder, templates::standard};
use std::sync::Arc;
use tokio::sync::Mutex;

pub fn builder() -> TemplateBuilder {
    Arc::new(|state: Arc<Mutex<IngestionState>>, deps: &PipelineDeps| {
        standard::builder()(state, deps)
    })
}
