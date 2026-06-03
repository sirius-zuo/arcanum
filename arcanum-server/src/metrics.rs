/// Initialize the metrics system with a Prometheus recorder.
/// Call this once at application startup.
///
/// This calls `metrics_prometheus::try_install()` which sets up the global
/// metrics recorder to use a Prometheus backend backed by
/// `prometheus::default_registry()`.
pub fn init_metrics() {
    let _ = metrics_prometheus::try_install();
}

/// Returns the current metrics in Prometheus text exposition format.
pub fn get_metrics_text() -> String {
    let encoder = prometheus::TextEncoder::new();
    let metrics = prometheus::default_registry().gather();
    match encoder.encode_to_string(&metrics) {
        Ok(text) => text,
        Err(e) => format!("# error encoding metrics: {e}"),
    }
}
