use crate::types::CollectionId;
use async_trait::async_trait;
use std::sync::Arc;

#[async_trait]
pub trait CacheInvalidator: Send + Sync {
    async fn invalidate_document(&self, source_uri: &str, collection_id: &CollectionId);
}

pub struct CacheInvalidationBroadcaster {
    invalidators: Vec<Arc<dyn CacheInvalidator>>,
}

impl CacheInvalidationBroadcaster {
    pub fn new(invalidators: Vec<Arc<dyn CacheInvalidator>>) -> Self {
        Self { invalidators }
    }

    pub async fn invalidate_document(&self, source_uri: &str, collection_id: &CollectionId) {
        let futs: Vec<_> = self.invalidators.iter()
            .map(|inv| {
                let inv = inv.clone();
                let uri = source_uri.to_string();
                let col = collection_id.clone();
                tokio::spawn(async move { inv.invalidate_document(&uri, &col).await })
            })
            .collect();
        for f in futs { let _ = f.await; }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::Mutex;

    struct RecordingInvalidator(Arc<Mutex<Vec<String>>>);

    #[async_trait]
    impl CacheInvalidator for RecordingInvalidator {
        async fn invalidate_document(&self, source_uri: &str, _collection_id: &CollectionId) {
            self.0.lock().await.push(source_uri.to_string());
        }
    }

    #[tokio::test]
    async fn test_broadcaster_calls_all_invalidators() {
        let log: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(vec![]));
        let a = Arc::new(RecordingInvalidator(log.clone()));
        let b = Arc::new(RecordingInvalidator(log.clone()));
        let broadcaster = CacheInvalidationBroadcaster::new(vec![a, b]);
        let col = CollectionId("my-collection".to_string());
        broadcaster.invalidate_document("file://doc.pdf", &col).await;
        let calls = log.lock().await;
        assert_eq!(calls.len(), 2);
        assert!(calls.iter().all(|s| s == "file://doc.pdf"));
    }

    #[tokio::test]
    async fn test_broadcaster_empty_is_noop() {
        let broadcaster = CacheInvalidationBroadcaster::new(vec![]);
        let col = CollectionId("x".to_string());
        broadcaster.invalidate_document("file://doc.pdf", &col).await;
    }
}
