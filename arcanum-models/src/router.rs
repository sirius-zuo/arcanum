use arcanum_core::{traits::*, types::*, Result};
use async_trait::async_trait;
use std::sync::{Arc, atomic::{AtomicUsize, Ordering}};

pub struct EmbeddingParallelismRouter {
    providers: Vec<Arc<dyn Embedder>>,
    counter: AtomicUsize,
}

impl EmbeddingParallelismRouter {
    pub fn new(providers: Vec<Arc<dyn Embedder>>) -> Self {
        assert!(!providers.is_empty(), "at least one embedder required");
        Self { providers, counter: AtomicUsize::new(0) }
    }
}

#[async_trait]
impl Embedder for EmbeddingParallelismRouter {
    async fn embed(&self, texts: Vec<String>) -> Result<Vec<Vector>> {
        let idx = self.counter.fetch_add(1, Ordering::Relaxed) % self.providers.len();
        self.providers[idx].embed(texts).await
    }

    fn dimension(&self) -> usize {
        self.providers[0].dimension()
    }
}
