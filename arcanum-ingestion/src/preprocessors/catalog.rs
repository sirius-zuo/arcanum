use arcanum_core::traits::Preprocessor;
use std::collections::HashMap;
use std::sync::Arc;

/// Name-keyed collection of preprocessors. Unlike the old MIME-keyed
/// PreprocessorRegistry, selection happens by a logical name chosen via
/// per-collection configuration, not by document MIME type — each
/// registered preprocessor is responsible for handling whichever MIME
/// types it supports internally (DoclingPreprocessor already does this).
pub struct PreprocessorCatalog {
    entries: HashMap<String, Arc<dyn Preprocessor>>,
}

impl PreprocessorCatalog {
    pub fn new() -> Self {
        Self { entries: HashMap::new() }
    }

    pub fn register(&mut self, name: impl Into<String>, p: Arc<dyn Preprocessor>) {
        self.entries.insert(name.into(), p);
    }

    pub fn get(&self, name: &str) -> Option<Arc<dyn Preprocessor>> {
        self.entries.get(name).cloned()
    }
}

impl Default for PreprocessorCatalog {
    fn default() -> Self {
        Self::new()
    }
}
