use crate::chunkers::{
    fixed::FixedSizeChunker,
    semantic::SemanticChunker,
    hierarchical::HierarchicalChunker,
    propositional::PropositionalChunker,
    structure::StructureAwareChunker,
};
use arcanum_core::{Result, ArcanumError, traits::Chunker, types::ChunkStrategyConfig};
use std::{collections::HashMap, sync::Arc};

type Factory = Box<dyn Fn(&serde_json::Value) -> Result<Arc<dyn Chunker>> + Send + Sync>;

pub struct ChunkRegistry {
    factories: HashMap<String, Factory>,
}

impl ChunkRegistry {
    pub fn new() -> Self {
        Self { factories: HashMap::new() }
    }

    pub fn register(
        &mut self,
        name: &str,
        factory: impl Fn(&serde_json::Value) -> Result<Arc<dyn Chunker>> + Send + Sync + 'static,
    ) {
        self.factories.insert(name.to_string(), Box::new(factory));
    }

    pub fn build(&self, config: &ChunkStrategyConfig) -> Result<Arc<dyn Chunker>> {
        let factory = self.factories.get(&config.strategy).ok_or_else(|| {
            ArcanumError::Config(format!("unknown chunk strategy '{}'", config.strategy))
        })?;
        factory(&config.params)
    }

    pub fn strategy_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.factories.keys().cloned().collect();
        names.sort();
        names
    }
}

pub fn default_registry() -> ChunkRegistry {
    let mut r = ChunkRegistry::new();

    r.register("fixed", |params| {
        let chunk_size = params["chunk_size"].as_u64().unwrap_or(512) as usize;
        let overlap    = params["overlap"].as_u64().unwrap_or(64) as usize;
        if overlap >= chunk_size {
            return Err(ArcanumError::Config(format!(
                "fixed chunker: overlap ({}) must be less than chunk_size ({})",
                overlap, chunk_size
            )));
        }
        Ok(Arc::new(FixedSizeChunker::new(chunk_size, overlap)))
    });

    r.register("semantic", |params| {
        let max_chars = params["max_chars"].as_u64().unwrap_or(1000) as usize;
        Ok(Arc::new(SemanticChunker::new(max_chars)))
    });

    r.register("hierarchical", |_| {
        Ok(Arc::new(HierarchicalChunker::new()))
    });

    r.register("propositional", |_| {
        Ok(Arc::new(PropositionalChunker::new()))
    });

    r.register("structure", |params| {
        let max_chunk_chars = params["max_chunk_chars"].as_u64().unwrap_or(2000) as usize;
        Ok(Arc::new(StructureAwareChunker::new(max_chunk_chars)))
    });

    r
}
