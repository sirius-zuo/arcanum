use async_trait::async_trait;

#[async_trait]
pub trait ProgressEmitter: Send + Sync {
    async fn emit(&self, event: &str, payload: serde_json::Value);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    struct RecordingEmitter(Arc<Mutex<Vec<(String, serde_json::Value)>>>);

    #[async_trait]
    impl ProgressEmitter for RecordingEmitter {
        async fn emit(&self, event: &str, payload: serde_json::Value) {
            self.0.lock().unwrap().push((event.to_string(), payload));
        }
    }

    #[tokio::test]
    async fn test_emitter_receives_events() {
        let log = Arc::new(Mutex::new(vec![]));
        let emitter = RecordingEmitter(log.clone());
        emitter
            .emit(
                "ingestion:progress",
                serde_json::json!({"status": "queued"}),
            )
            .await;
        let entries = log.lock().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].0, "ingestion:progress");
    }
}
