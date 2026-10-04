/// Returns the current metrics in Prometheus text exposition format.
pub fn get_metrics_text() -> String {
    let encoder = prometheus::TextEncoder::new();
    let metrics = prometheus::default_registry().gather();
    match encoder.encode_to_string(&metrics) {
        Ok(text) => text,
        Err(e) => format!("# error encoding metrics: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::get_metrics_text;

    /// The recorder installed by `metrics-prometheus` must write into the same
    /// registry `get_metrics_text` encodes. A version split between the two
    /// `prometheus` crates once left `/metrics` permanently empty.
    #[test]
    fn recorded_metrics_appear_in_exposition_text() {
        // Another test in this binary may have installed a recorder already.
        let _ = metrics_prometheus::try_install();
        metrics::counter!("arcanum_metrics_registry_probe_total").increment(1);
        let text = get_metrics_text();
        assert!(
            text.contains("arcanum_metrics_registry_probe_total"),
            "recorded metric missing from /metrics output: {text:?}"
        );
    }
}
