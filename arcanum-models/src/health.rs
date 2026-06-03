use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

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
}
