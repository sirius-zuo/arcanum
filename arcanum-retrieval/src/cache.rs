use arcanum_core::types::*;
use std::{collections::HashMap, sync::RwLock, time::{Duration, Instant}};

struct CacheEntry { result: RetrievalResult, inserted: Instant }

pub struct QueryCache {
    store: RwLock<HashMap<String, CacheEntry>>,
    ttl: Duration,
    max_size: usize,
}

impl QueryCache {
    pub fn new(max_size: usize, ttl: Duration) -> Self {
        Self { store: RwLock::new(HashMap::new()), ttl, max_size }
    }

    pub fn get(&self, key: &str) -> Option<RetrievalResult> {
        let store = self.store.read().unwrap();
        let entry = store.get(key)?;
        if entry.inserted.elapsed() > self.ttl { return None; }
        Some(entry.result.clone())
    }

    pub fn insert(&self, key: String, result: RetrievalResult) {
        let mut store = self.store.write().unwrap();
        if store.len() >= self.max_size {
            if let Some(oldest) = store.iter()
                .min_by_key(|(_, v)| v.inserted)
                .map(|(k, _)| k.clone())
            {
                store.remove(&oldest);
            }
        }
        store.insert(key, CacheEntry { result, inserted: Instant::now() });
    }

    pub fn cache_key(query: &Query) -> String {
        format!("{}:{}:{}", query.text,
            query.collection_id.as_ref().map(|c| c.0.as_str()).unwrap_or(""),
            query.top_k)
    }
}
