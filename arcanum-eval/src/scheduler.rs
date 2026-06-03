use std::time::Duration;
use tracing::debug;

pub struct EvalScheduler {
    pub interval_secs: u64,
}

impl EvalScheduler {
    pub fn new(interval: Duration) -> Self {
        Self { interval_secs: interval.as_secs() }
    }

    pub fn start<F, Fut>(self, run_eval_fn: F) -> tokio::task::JoinHandle<()>
    where
        F: Fn() -> Fut + Send + 'static,
        Fut: std::future::Future<Output = ()> + Send,
    {
        debug!(interval_secs = self.interval_secs, "EvalScheduler starting");
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(self.interval_secs));
            loop {
                interval.tick().await;
                run_eval_fn().await;
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_eval_scheduler_construction() {
        let scheduler = EvalScheduler::new(Duration::from_secs(3600));
        assert_eq!(scheduler.interval_secs, 3600);
    }
}
