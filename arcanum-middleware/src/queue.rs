use arcanum_core::{Result, ArcanumError};
use tokio::sync::mpsc;

pub struct BoundedQueue<T> {
    tx: mpsc::Sender<T>,
    rx: tokio::sync::Mutex<mpsc::Receiver<T>>,
}

impl<T: Send + 'static> BoundedQueue<T> {
    pub fn new(capacity: usize) -> Self {
        let (tx, rx) = mpsc::channel(capacity);
        Self { tx, rx: tokio::sync::Mutex::new(rx) }
    }

    pub async fn push(&self, item: T) -> Result<()> {
        self.tx.try_send(item).map_err(|_| ArcanumError::QueueFull)
    }

    pub async fn pop(&self) -> Option<T> {
        self.rx.lock().await.recv().await
    }
}
