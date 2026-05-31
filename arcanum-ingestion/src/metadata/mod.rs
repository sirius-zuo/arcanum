use sha2::{Sha256, Digest};
use std::collections::HashMap;
use tokio::sync::RwLock;

pub mod title;
pub mod keyword;
pub mod hierarchy;
pub use title::extract_title;
pub use keyword::extract_keywords;
pub use hierarchy::extract_hierarchy;

pub struct DocumentHashTracker {
    store: RwLock<HashMap<String, String>>,
}

impl DocumentHashTracker {
    pub fn new() -> Self {
        Self { store: RwLock::new(HashMap::new()) }
    }

    pub fn compute_hash(content: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(content);
        hex::encode(hasher.finalize())
    }

    pub async fn ever_seen(&self, uri: &str) -> bool {
        self.store.read().await.contains_key(uri)
    }

    pub async fn seen_unchanged(&self, uri: &str, content: &[u8]) -> bool {
        let hash = Self::compute_hash(content);
        self.store.read().await.get(uri).map_or(false, |h| h == &hash)
    }

    pub async fn record(&self, uri: &str, content: &[u8]) {
        let hash = Self::compute_hash(content);
        self.store.write().await.insert(uri.to_string(), hash);
    }
}
