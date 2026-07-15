/// Returns the current metrics in Prometheus text exposition format.
pub fn get_metrics_text() -> String {
    let encoder = prometheus::TextEncoder::new();
    let metrics = prometheus::default_registry().gather();
    match encoder.encode_to_string(&metrics) {
        Ok(text) => text,
        Err(e) => format!("# error encoding metrics: {e}"),
    }
}
