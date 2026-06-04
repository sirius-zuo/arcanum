pub mod config;
pub mod init;
#[cfg(feature = "testing-helpers")]
pub mod testing;

pub use config::TelemetryConfig;
pub use init::{init, TelemetryGuard};

#[cfg(test)]
mod tests {
    use crate::config::{LogFormat, OtlpProtocol, TelemetryConfig};
    use crate::init;

    #[test]
    fn default_config_does_not_panic_on_init() {
        let config = TelemetryConfig {
            log_filter:    "off".into(),
            log_format:    LogFormat::Pretty,
            otlp_endpoint: None,
            otlp_protocol: OtlpProtocol::Grpc,
            service_name:  "test".into(),
            metrics_enabled: false,
            metrics_otlp:    false,
            metrics_token:   None,
        };
        let _guard = init(config);
        // No panic = success
    }
}
