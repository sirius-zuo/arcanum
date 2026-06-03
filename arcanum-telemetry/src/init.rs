use crate::config::{LogFormat, TelemetryConfig};
use opentelemetry_sdk::trace::TracerProvider;
use tracing_subscriber::{util::SubscriberInitExt, EnvFilter};

pub struct TelemetryGuard {
    pub(crate) tracer_provider: Option<TracerProvider>,
    pub(crate) prometheus_handle: Option<metrics_exporter_prometheus::PrometheusHandle>,
}

impl TelemetryGuard {
    /// Returns the Prometheus handle used to render `/metrics` output.
    /// `None` when `ARCANUM_METRICS_ENABLED=false`.
    pub fn prometheus_handle(&self) -> Option<&metrics_exporter_prometheus::PrometheusHandle> {
        self.prometheus_handle.as_ref()
    }
}

impl Drop for TelemetryGuard {
    fn drop(&mut self) {
        if let Some(provider) = self.tracer_provider.take() {
            if let Err(e) = provider.shutdown() {
                eprintln!("arcanum-telemetry: failed to flush traces on shutdown: {e:?}");
            }
        }
    }
}

pub fn init(config: TelemetryConfig) -> TelemetryGuard {
    use tracing_subscriber::layer::SubscriberExt;

    // Build OTLP tracer provider only when an endpoint is configured.
    // `build_tracer_provider` is called later in Task 3; for now it's todo!().
    let tracer_provider = config
        .otlp_endpoint
        .as_deref()
        .map(|_| build_tracer_provider(&config));

    // ── Subscriber layers ────────────────────────────────────────────────
    // Both fmt variants are wrapped in Option so they can be added via
    // `.with()` unconditionally — Option<L> implements Layer as a no-op.
    let json_fmt = if matches!(config.log_format, LogFormat::Json) {
        Some(tracing_subscriber::fmt::layer().json())
    } else {
        None
    };
    let plain_fmt = if matches!(config.log_format, LogFormat::Pretty) {
        Some(tracing_subscriber::fmt::layer())
    } else {
        None
    };
    let otel_layer = tracer_provider.as_ref().map(|provider| {
        use opentelemetry::trace::TracerProvider as _;
        tracing_opentelemetry::OpenTelemetryLayer::new(provider.tracer("arcanum"))
            .with_error_events_to_exceptions(true)
            .with_error_events_to_status(true)
    });

    // try_init (not init) so multiple test invocations don't panic.
    let _ = tracing_subscriber::registry()
        .with(EnvFilter::new(&config.log_filter))
        .with(json_fmt)
        .with(plain_fmt)
        .with(otel_layer)
        .try_init();

    // ── Panic hook ───────────────────────────────────────────────────────
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        tracing::error!(panic.info = %info, "process panicked");
        prev(info);
    }));

    // ── Prometheus metrics recorder ──────────────────────────────────────
    // install_recorder() is idempotent-ish via .ok() — silently no-ops if
    // already installed (avoids panics when tests call init() multiple times).
    let prometheus_handle = if config.metrics_enabled {
        metrics_exporter_prometheus::PrometheusBuilder::new()
            .install_recorder()
            .ok()
    } else {
        None
    };

    TelemetryGuard { tracer_provider, prometheus_handle }
}

fn build_tracer_provider(_config: &TelemetryConfig) -> TracerProvider {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{LogFormat, OtlpProtocol};
    use serial_test::serial;

    fn silent_local_config() -> TelemetryConfig {
        TelemetryConfig {
            log_filter:       "off".into(),
            log_format:       LogFormat::Pretty,
            otlp_endpoint:    None,
            otlp_protocol:    OtlpProtocol::Grpc,
            service_name:     "test-arcanum".into(),
            metrics_enabled:  false,
            metrics_otlp:     false,
            metrics_token:    None,
        }
    }

    #[test]
    #[serial]
    fn init_local_mode_no_tracer_provider() {
        let guard = init(silent_local_config());
        assert!(guard.tracer_provider.is_none(),
            "local mode should not set up a tracer provider");
    }

    #[test]
    #[serial]
    fn init_metrics_disabled_no_prometheus_handle() {
        let guard = init(silent_local_config());
        assert!(guard.prometheus_handle().is_none(),
            "metrics_enabled=false should produce no prometheus handle");
    }

    #[test]
    #[serial]
    fn guard_drops_cleanly_in_local_mode() {
        let guard = init(silent_local_config());
        drop(guard); // must not panic
    }
}
