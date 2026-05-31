use arcanum_core::traits::ProgressEmitter;
use serde_json::Value;
use std::collections::HashMap;
use tokio::sync::{broadcast, RwLock};

#[derive(Debug)]
pub struct EventBus {
    senders: RwLock<HashMap<String, broadcast::Sender<Value>>>,
}

impl EventBus {
    pub fn new() -> Self {
        Self { senders: RwLock::new(HashMap::new()) }
    }

    pub async fn subscribe(&self, topic: &str) -> broadcast::Receiver<Value> {
        let mut map = self.senders.write().await;
        let sender = map.entry(topic.to_string())
            .or_insert_with(|| broadcast::channel(128).0);
        sender.subscribe()
    }

    pub async fn publish(&self, topic: &str, payload: Value) {
        let map = self.senders.read().await;
        if let Some(sender) = map.get(topic) {
            let _ = sender.send(payload);
        }
    }
}

#[async_trait::async_trait]
impl ProgressEmitter for EventBus {
    async fn emit(&self, event: &str, payload: serde_json::Value) {
        self.publish(event, payload).await;
    }
}
