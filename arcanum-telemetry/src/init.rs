use crate::config::{LogFormat, OtlpProtocol, TelemetryConfig};
use opentelemetry_sdk::trace::TracerProvider;
use tracing_subscriber::{util::SubscriberInitExt, EnvFilter};
use std::sync::OnceLock;

// Guards the panic-hook installation so repeated init() calls (e.g. in tests)
// never chain hooks.
static PANIC_HOOK_INSTALLED: OnceLock<()> = OnceLock::new();

pub struct TelemetryGuard {
    pub(crate) prometheus_handle: Option<metrics_exporter_prometheus::PrometheusHandle>,
}

impl TelemetryGuard {
    /// Returns the Prometheus handle used to render `/metrics` output.
    /// `None` when `ARCANUM_METRICS_ENABLED=false` or when the recorder
    /// could not be installed.
    pub fn prometheus_handle(&self) -> Option<&metrics_exporter_prometheus::PrometheusHandle> {
        self.prometheus_handle.as_ref()
    }
}

impl Drop for TelemetryGuard {
    fn drop(&mut self) {
        // Shut down through the global so the termination signal propagates
        // via the same Arc that the OTel layer's Tracer holds. This ensures
        // correct ordering: the layer sees the provider go into shutdown state
        // before the underlying data is freed.
        opentelemetry::global::shutdown_tracer_provider();
    }
}

pub fn init(config: TelemetryConfig) -> TelemetryGuard {
    use tracing_subscriber::layer::SubscriberExt;

    // ── C9: warn about config fields that are parsed but not yet wired ────────
    if config.metrics_otlp {
        eprintln!(
            "arcanum-telemetry: ARCANUM_METRICS_OTLP=true is set but OTLP \
             metrics push is not implemented until Stage 6. Metrics are only \
             available via the /metrics scrape endpoint."
        );
    }
    if config.metrics_token.is_some() {
        eprintln!(
            "arcanum-telemetry: ARCANUM_METRICS_TOKEN is set but bearer-token \
             enforcement is not implemented until Stage 6. The /metrics \
             endpoint is currently unprotected."
        );
    }

    // ── C3: build OTel provider before subscriber install (layer must be
    // attached in the same try_init call), degrading gracefully on failure ─────
    let tracer_provider: Option<TracerProvider> = config
        .otlp_endpoint
        .as_deref()
        .and_then(|_| match build_tracer_provider(&config) {
            Ok(p) => Some(p),
            Err(e) => {
                eprintln!(
                    "arcanum-telemetry: OTLP exporter failed to build, \
                     running without distributed tracing: {e:?}"
                );
                None
            }
        });

    // ── Subscriber layers ─────────────────────────────────────────────────────
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

    // ── C2: handle try_init() failure — log but do not crash ─────────────────
    // try_init returns Err when a global subscriber is already installed.
    // eprintln! is used here (not tracing!) because tracing is not yet set up.
    if tracing_subscriber::registry()
        .with(EnvFilter::new(&config.log_filter))
        .with(json_fmt)
        .with(plain_fmt)
        .with(otel_layer)
        .try_init()
        .is_err()
    {
        eprintln!(
            "arcanum-telemetry: a tracing subscriber is already installed; \
             OTLP, log format, and filter settings were not applied."
        );
    }

    // ── C8: register provider via global so Drop's shutdown_tracer_provider()
    // propagates through the same Arc the OTel layer's Tracer holds ────────────
    if let Some(ref p) = tracer_provider {
        opentelemetry::global::set_tracer_provider(p.clone());
    }

    // ── C1: install panic hook exactly once via OnceLock ─────────────────────
    PANIC_HOOK_INSTALLED.get_or_init(|| {
        let prev = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            tracing::error!(panic.info = %info, "process panicked");
            prev(info);
        }));
    });

    // ── C7: log a warning when the Prometheus recorder cannot be installed ─────
    let prometheus_handle = if config.metrics_enabled {
        match metrics_exporter_prometheus::PrometheusBuilder::new().install_recorder() {
            Ok(handle) => Some(handle),
            Err(e) => {
                tracing::warn!(
                    err = ?e,
                    "failed to install Prometheus recorder — /metrics will \
                     not render data; a different recorder may already be installed"
                );
                None
            }
        }
    } else {
        None
    };

    TelemetryGuard { prometheus_handle }
}

fn build_tracer_provider(
    config: &TelemetryConfig,
) -> Result<TracerProvider, Box<dyn std::error::Error + Send + Sync>> {
    use opentelemetry::KeyValue;
    use opentelemetry_sdk::{resource::Resource, runtime::Tokio};

    // Resource::default() reads OTEL_RESOURCE_ATTRIBUTES and the SDK's
    // built-in detectors (telemetry.sdk.name, telemetry.sdk.language, etc.).
    // Merging our service.name on top ensures it takes precedence over any
    // value that may already exist in the default resource.
    let resource = Resource::default().merge(&Resource::new(vec![KeyValue::new(
        "service.name",
        config.service_name.clone(),
    )]));

    let exporter = match config.otlp_protocol {
        OtlpProtocol::Grpc => opentelemetry_otlp::SpanExporter::builder()
            .with_tonic()
            .build()
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?,
        OtlpProtocol::HttpProtobuf => opentelemetry_otlp::SpanExporter::builder()
            .with_http()
            .build()
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?,
    };

    Ok(TracerProvider::builder()
        .with_resource(resource)
        .with_batch_exporter(exporter, Tokio)
        .build())
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
    fn init_local_mode_guard_has_no_prometheus_handle() {
        let guard = init(silent_local_config());
        assert!(guard.prometheus_handle().is_none(),
            "metrics_enabled=false → no prometheus handle");
    }

    #[test]
    #[serial]
    fn init_called_twice_does_not_panic() {
        // Both the panic hook (OnceLock) and try_init() (silently ignores
        // second registration) must be idempotent. The second guard should
        // return cleanly with a None prometheus handle (metrics_enabled=false).
        let _first  = init(silent_local_config());
        let second = init(silent_local_config());
        drop(_first);
        drop(second);
        // No panic = success
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

    #[test]
    #[serial]
    fn init_with_fake_otlp_endpoint_does_not_panic() {
        // Sets a non-reachable endpoint. No spans are exported (batch exporter
        // will silently drop them), but provider setup must not panic.
        let rt = tokio::runtime::Runtime::new().unwrap();
        let _enter = rt.enter();
        let guard = init(TelemetryConfig {
            log_filter:    "off".into(),
            log_format:    LogFormat::Pretty,
            otlp_endpoint: Some("http://127.0.0.1:14317".into()), // nothing listening here
            otlp_protocol: OtlpProtocol::Grpc,
            service_name:  "test-otlp".into(),
            metrics_enabled: false,
            metrics_otlp:    false,
            metrics_token:   None,
        });
        drop(guard); // returning TelemetryGuard without panic = success
    }
}
