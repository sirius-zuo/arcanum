use arcanum_core::{traits::CacheInvalidator, types::*};
use async_trait::async_trait;
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

#[async_trait]
impl CacheInvalidator for QueryCache {
    /// Remove all cached entries whose key contains the collection_id.
    ///
    /// Cache keys are formatted as `"{query_text}:{collection_id}:{top_k}"`,
    /// so filtering on `collection_id.0` reliably scopes eviction to one collection.
    async fn invalidate_document(&self, _source_uri: &str, collection_id: &CollectionId) {
        let mut store = self.store.write().unwrap();
        store.retain(|key, _| !key.contains(&collection_id.0));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_result() -> RetrievalResult {
        RetrievalResult {
            chunks: vec![],
            citations: vec![],
            strategy_scores: Default::default(),
            confidence: 1.0,
        }
    }

    #[test]
    fn test_cache_insert_and_get() {
        let cache = QueryCache::new(10, Duration::from_secs(60));
        cache.insert("key1".into(), dummy_result());
        assert!(cache.get("key1").is_some());
        assert!(cache.get("key2").is_none());
    }

    #[tokio::test]
    async fn test_cache_invalidate_removes_collection_entries() {
        let cache = QueryCache::new(100, Duration::from_secs(60));
        let col_a = CollectionId("col-alpha".into());
        let col_b = CollectionId("col-beta".into());

        // Insert entries for both collections using the standard cache_key format.
        let q_a = Query::new("query a").with_collection(col_a.clone());
        let q_b = Query::new("query b").with_collection(col_b.clone());
        cache.insert(QueryCache::cache_key(&q_a), dummy_result());
        cache.insert(QueryCache::cache_key(&q_b), dummy_result());

        assert!(cache.get(&QueryCache::cache_key(&q_a)).is_some(), "col-alpha entry should exist before invalidation");
        assert!(cache.get(&QueryCache::cache_key(&q_b)).is_some(), "col-beta entry should exist before invalidation");

        // Invalidate only col-alpha.
        cache.invalidate_document("file://doc.pdf", &col_a).await;

        assert!(cache.get(&QueryCache::cache_key(&q_a)).is_none(),
            "col-alpha entry should be gone after invalidation");
        assert!(cache.get(&QueryCache::cache_key(&q_b)).is_some(),
            "col-beta entry should remain after col-alpha invalidation");
    }
}
