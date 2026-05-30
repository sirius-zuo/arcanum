use crate::{dag::PipelineDAG, deps::PipelineDeps, ingestion_state::IngestionState};
use arcanum_core::{Result, ArcanumError};
use std::{collections::HashMap, sync::Arc};
use tokio::sync::Mutex;

pub type TemplateBuilder = Arc<
    dyn Fn(Arc<Mutex<IngestionState>>, &PipelineDeps) -> PipelineDAG + Send + Sync
>;

pub struct ArcanumPipelineRegistry {
    builders: HashMap<String, TemplateBuilder>,
}

impl ArcanumPipelineRegistry {
    pub fn new() -> Self { Self { builders: HashMap::new() } }

    pub fn register(&mut self, name: &str, builder: TemplateBuilder) {
        self.builders.insert(name.to_string(), builder);
    }

    pub fn build(
        &self, name: &str,
        state: Arc<Mutex<IngestionState>>,
        deps: &PipelineDeps,
    ) -> Result<PipelineDAG> {
        self.builders.get(name)
            .ok_or_else(|| ArcanumError::Pipeline {
                stage: "registry".into(),
                message: format!("unknown pipeline template: '{name}'"),
            })
            .map(|builder| builder(state, deps))
    }
}

impl Default for ArcanumPipelineRegistry {
    fn default() -> Self {
        let mut r = Self::new();
        r.register("standard",   crate::templates::standard::builder());
        r.register("contextual", crate::templates::contextual::builder());
        r.register("graph",      crate::templates::graph::builder());
        r.register("raptor",     crate::templates::raptor::builder());
        r.register("full",       crate::templates::full::builder());
        r
    }
}
