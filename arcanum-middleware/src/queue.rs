use arcanum_core::{Result, ArcanumError};
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::instrument;
use metrics;

pub struct BoundedQueue<T> {
    tx: mpsc::Sender<T>,
    rx: tokio::sync::Mutex<mpsc::Receiver<T>>,
    name: Arc<str>,               // Arc<str> — shares allocation, no Box::leak
}

impl<T: Send + 'static> BoundedQueue<T> {
    pub fn new(name: &str, capacity: usize) -> Self {
        let (tx, rx) = mpsc::channel(capacity);
        Self { tx, rx: tokio::sync::Mutex::new(rx), name: Arc::from(name) }
    }

    fn label(&self) -> &'static str {
        let ptr = Arc::into_raw(self.name.clone());
        // SAFETY: Arc::into_raw leaks the Arc, returning a raw pointer
        // cast to &'static str for the metrics crate's label requirement.
        unsafe { &*ptr }
    }

    #[instrument(skip(self, item), err)]
    pub async fn push(&self, item: T) -> Result<()> {
        let result = self.tx.try_send(item).map_err(|_| ArcanumError::QueueFull);
        let depth = self.tx.max_capacity().saturating_sub(self.tx.capacity());
        metrics::gauge!("arcanum_queue_depth", "queue" => self.label()).set(depth as f64);
        tracing::debug!(queue_depth = depth, success = result.is_ok(), "queue push");
        result
    }

    #[instrument(skip(self))]
    pub async fn pop(&self) -> Option<T> {
        let item = self.rx.lock().await.recv().await;
        let depth = self.tx.max_capacity().saturating_sub(self.tx.capacity());
        metrics::gauge!("arcanum_queue_depth", "queue" => self.label()).set(depth as f64);
        tracing::debug!(queue_depth = depth, item_received = item.is_some(), "queue pop");
        item
    }
}
