// arcanum-telemetry/src/testing.rs
//
// Utilities for asserting telemetry in integration tests.

use opentelemetry_sdk::{
    export::trace::SpanData,
    testing::trace::InMemorySpanExporter,
    trace::TracerProvider as SdkTracerProvider,
};

/// A test telemetry setup that captures all spans in memory.
pub struct TestTelemetry {
    pub exporter: InMemorySpanExporter,
    pub provider: SdkTracerProvider,
}

impl TestTelemetry {
    /// Install a tracing subscriber that sends all spans to an in-memory exporter.
    /// Returns the handle so callers can drain and inspect spans.
    /// Uses `try_init` — safe to call from multiple tests.
    pub fn install() -> Self {
        use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

        let exporter = InMemorySpanExporter::default();
        let provider = SdkTracerProvider::builder()
            .with_simple_exporter(exporter.clone())
            .build();

        let tracer = opentelemetry::trace::TracerProvider::tracer(&provider, "arcanum-test");
        let otel_layer = tracing_opentelemetry::OpenTelemetryLayer::new(tracer)
            .with_error_events_to_exceptions(true)
            .with_error_events_to_status(true);

        let _ = tracing_subscriber::registry()
            .with(tracing_subscriber::filter::LevelFilter::DEBUG)
            .with(otel_layer)
            .try_init();

        Self { exporter, provider }
    }

    /// Flush and return all spans collected since `install()` was called.
    pub fn drain_spans(&self) -> Vec<SpanData> {
        self.provider.force_flush();
        self.exporter.get_finished_spans().unwrap_or_default()
    }

    /// Return `true` if any collected span has the given name.
    pub fn has_span_named(&self, name: &str) -> bool {
        self.drain_spans().iter().any(|s| s.name == name)
    }

    /// Return all span names collected so far.
    pub fn span_names(&self) -> Vec<String> {
        self.drain_spans().iter().map(|s| s.name.to_string()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn install_returns_telemetry_handle() {
        let telem = TestTelemetry::install();
        // Emit a test span
        tracing::info_span!("test.span").in_scope(|| {});
        let spans = telem.drain_spans();
        assert!(spans.iter().any(|s| s.name == "test.span"),
            "expected test.span in {:?}", telem.span_names());
    }
}
