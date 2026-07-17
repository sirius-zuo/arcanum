use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use arcanum_core::{traits::Embedder, types::Vector, Result};
use async_trait::async_trait;

#[derive(Debug, Clone)]
pub struct ProviderStats {
    pub provider_id: String,
    pub total_calls: u64,
    pub error_count: u64,
    pub error_rate: f64,
    pub avg_latency_ms: f64,
}

pub struct ProviderHealthMonitor {
    provider_id: String,
    total_calls: AtomicU64,
    error_count: AtomicU64,
    total_latency_ms: AtomicU64,
}

impl ProviderHealthMonitor {
    pub fn new(provider_id: &str) -> Arc<Self> {
        Arc::new(Self {
            provider_id: provider_id.to_string(),
            total_calls: AtomicU64::new(0),
            error_count: AtomicU64::new(0),
            total_latency_ms: AtomicU64::new(0),
        })
    }

    pub fn record_success(&self, latency: Duration) {
        self.total_calls.fetch_add(1, Ordering::Relaxed);
        self.total_latency_ms.fetch_add(latency.as_millis() as u64, Ordering::Relaxed);
        tracing::debug!(
            provider_id = %self.provider_id,
            latency_ms = latency.as_millis(),
            "model provider success"
        );
    }

    pub fn record_error(&self) {
        self.total_calls.fetch_add(1, Ordering::Relaxed);
        self.error_count.fetch_add(1, Ordering::Relaxed);
        tracing::warn!(provider_id = %self.provider_id, "model provider error recorded");
    }

    pub fn stats(&self) -> ProviderStats {
        let total = self.total_calls.load(Ordering::Relaxed);
        let errors = self.error_count.load(Ordering::Relaxed);
        let latency_sum = self.total_latency_ms.load(Ordering::Relaxed);
        let success_calls = total.saturating_sub(errors);
        ProviderStats {
            provider_id: self.provider_id.clone(),
            total_calls: total,
            error_count: errors,
            error_rate: if total > 0 { errors as f64 / total as f64 } else { 0.0 },
            avg_latency_ms: if success_calls > 0 { latency_sum as f64 / success_calls as f64 } else { 0.0 },
        }
    }
}

/// Observation-only decorator: feeds every embed call's outcome and latency
/// into a ProviderHealthMonitor. Results and errors pass through unchanged.
pub struct MonitoredEmbedder {
    inner:   Arc<dyn Embedder>,
    monitor: Arc<ProviderHealthMonitor>,
}

impl MonitoredEmbedder {
    pub fn new(inner: Arc<dyn Embedder>, provider_id: &str) -> Self {
        Self { inner, monitor: ProviderHealthMonitor::new(provider_id) }
    }
    pub fn monitor(&self) -> &Arc<ProviderHealthMonitor> { &self.monitor }
}

#[async_trait]
impl Embedder for MonitoredEmbedder {
    async fn embed(&self, texts: Vec<String>) -> Result<Vec<Vector>> {
        let start = std::time::Instant::now();
        let provider_id = self.monitor.provider_id.clone();
        match self.inner.embed(texts).await {
            Ok(v)  => {
                self.monitor.record_success(start.elapsed());
                metrics::counter!("arcanum_model_provider_calls_total", "provider" => provider_id.clone(), "status" => "ok").increment(1);
                Ok(v)
            }
            Err(e) => {
                self.monitor.record_error();
                metrics::counter!("arcanum_model_provider_calls_total", "provider" => provider_id.clone(), "status" => "error").increment(1);
                Err(e)
            }
        }
    }
    fn dimension(&self) -> usize { self.inner.dimension() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_health_monitor_records_latency() {
        let monitor = ProviderHealthMonitor::new("ollama");
        monitor.record_success(Duration::from_millis(120));
        monitor.record_success(Duration::from_millis(80));
        let stats = monitor.stats();
        assert!(stats.avg_latency_ms > 0.0);
        assert_eq!(stats.total_calls, 2);
        assert_eq!(stats.error_count, 0);
    }

    #[test]
    fn test_health_monitor_records_errors() {
        let monitor = ProviderHealthMonitor::new("openai");
        monitor.record_error();
        monitor.record_error();
        monitor.record_success(Duration::from_millis(50));
        let stats = monitor.stats();
        assert_eq!(stats.error_count, 2);
        assert_eq!(stats.total_calls, 3);
        assert!((stats.error_rate - 2.0 / 3.0).abs() < 0.001);
    }

    #[tokio::test]
    async fn monitored_embedder_records_success_and_error() {
        struct FlakyEmbedder(std::sync::atomic::AtomicUsize);
        #[async_trait::async_trait]
        impl arcanum_core::traits::Embedder for FlakyEmbedder {
            async fn embed(&self, texts: Vec<String>) -> arcanum_core::Result<Vec<arcanum_core::types::Vector>> {
                if self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 {
                    Err(arcanum_core::ArcanumError::Config("provider down".into()))
                } else {
                    Ok(texts.iter().map(|_| arcanum_core::types::Vector(vec![0.1])).collect())
                }
            }
            fn dimension(&self) -> usize { 1 }
        }
        let inner = std::sync::Arc::new(FlakyEmbedder(Default::default()));
        let me = MonitoredEmbedder::new(inner, "test-provider");
        assert!(me.embed(vec!["a".into()]).await.is_err());
        assert!(me.embed(vec!["a".into()]).await.is_ok());
        let stats = me.monitor().stats();
        assert_eq!(stats.total_calls, 2);
        assert_eq!(stats.error_count, 1);
        assert!(stats.avg_latency_ms >= 0.0);
    }
}
