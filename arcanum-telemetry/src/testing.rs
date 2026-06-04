// arcanum-telemetry/src/testing.rs
//
// Utilities for asserting telemetry in integration tests.
// Exposed only when the `testing-helpers` feature is enabled (or in test builds).

use opentelemetry::KeyValue;
use opentelemetry_sdk::{
    export::trace::SpanData,
    resource::Resource,
    testing::trace::InMemorySpanExporter,
    trace::TracerProvider as SdkTracerProvider,
};

/// A test telemetry setup that captures all spans in memory.
///
/// Call `install()` at the start of each test, then `get_spans()` after
/// exercising the code under test. The subscriber is scoped to this struct's
/// lifetime: when `TestTelemetry` is dropped, the previous subscriber is
/// restored and `provider.shutdown()` is called to flush any in-flight spans.
///
/// **Important:** use `#[serial]` on any test that creates a `TestTelemetry`
/// to prevent parallel tests on the same OS thread from interleaving guards.
pub struct TestTelemetry {
    pub exporter: InMemorySpanExporter,
    pub provider: SdkTracerProvider,
    // Held for RAII: restores the previous thread-local subscriber on drop.
    _subscriber_guard: tracing::subscriber::DefaultGuard,
}

impl TestTelemetry {
    /// Install a tracing subscriber that sends all spans to an in-memory exporter.
    ///
    /// The subscriber is thread-local (`set_default`). Tests decorated with
    /// `#[tokio::test]` use a `current_thread` runtime by default, so all
    /// futures run on the calling OS thread and see this subscriber.
    pub fn install() -> Self {
        use tracing_subscriber::{filter::LevelFilter, layer::SubscriberExt, util::SubscriberInitExt};

        let exporter = InMemorySpanExporter::default();
        let resource = Resource::new(vec![KeyValue::new("service.name", "arcanum-test")]);
        let provider = SdkTracerProvider::builder()
            .with_simple_exporter(exporter.clone())
            .with_resource(resource)
            .build();

        let tracer = opentelemetry::trace::TracerProvider::tracer(&provider, "arcanum-test");
        let otel_layer = tracing_opentelemetry::OpenTelemetryLayer::new(tracer)
            .with_error_events_to_exceptions(true)
            .with_error_events_to_status(true);

        let subscriber = tracing_subscriber::registry()
            .with(LevelFilter::DEBUG)
            .with(otel_layer);

        let guard = subscriber.set_default();

        Self {
            exporter,
            provider,
            _subscriber_guard: guard,
        }
    }

    /// Flush pending spans and return all spans collected since `install()`.
    ///
    /// The exporter buffer is not cleared between calls — all spans accumulated
    /// since `install()` are always returned. Call this once and reuse the
    /// `Vec` across multiple assertions.
    pub fn get_spans(&self) -> Vec<SpanData> {
        let _ = self.provider.force_flush();
        self.exporter.get_finished_spans().unwrap_or_default()
    }

    /// Return `true` if any collected span has the given name.
    pub fn has_span_named(&self, name: &str) -> bool {
        self.get_spans().iter().any(|s| s.name == name)
    }

    /// Return all span names collected so far (useful in assert error messages).
    pub fn span_names(&self) -> Vec<String> {
        self.get_spans().iter().map(|s| s.name.to_string()).collect()
    }
}

impl Drop for TestTelemetry {
    fn drop(&mut self) {
        let _ = self.provider.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn install_returns_telemetry_handle() {
        let telem = TestTelemetry::install();
        tracing::info_span!("test.span").in_scope(|| {});
        let spans = telem.get_spans();
        assert!(
            spans.iter().any(|s| s.name == "test.span"),
            "expected test.span in {:?}", telem.span_names()
        );
    }


}
