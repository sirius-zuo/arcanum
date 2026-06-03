use arcanum_core::{Result, ArcanumError};
use tokio::sync::mpsc;
use tracing::instrument;

pub struct BoundedQueue<T> {
    tx: mpsc::Sender<T>,
    rx: tokio::sync::Mutex<mpsc::Receiver<T>>,
}

impl<T: Send + 'static> BoundedQueue<T> {
    pub fn new(capacity: usize) -> Self {
        let (tx, rx) = mpsc::channel(capacity);
        Self { tx, rx: tokio::sync::Mutex::new(rx) }
    }

    #[instrument(skip(self, item), err)]
    pub async fn push(&self, item: T) -> Result<()> {
        let result = self.tx.try_send(item).map_err(|_| ArcanumError::QueueFull);
        let depth = self.tx.max_capacity().saturating_sub(self.tx.capacity());
        tracing::debug!(queue_depth = depth, success = result.is_ok(), "queue push");
        result
    }

    #[instrument(skip(self))]
    pub async fn pop(&self) -> Option<T> {
        let item = self.rx.lock().await.recv().await;
        let depth = self.tx.max_capacity().saturating_sub(self.tx.capacity());
        tracing::debug!(queue_depth = depth, item_received = item.is_some(), "queue pop");
        item
    }
}
