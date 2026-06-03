#[derive(Debug, Clone, PartialEq)]
pub enum LogFormat { Pretty, Json }

#[derive(Debug, Clone, PartialEq)]
pub enum OtlpProtocol { Grpc, HttpProtobuf }

#[derive(Debug, Clone)]
pub struct TelemetryConfig {
    pub log_filter: String,
    pub log_format: LogFormat,
    pub otlp_endpoint: Option<String>,
    pub otlp_protocol: OtlpProtocol,
    pub service_name: String,
    pub metrics_enabled: bool,
    pub metrics_otlp: bool,
    pub metrics_token: Option<String>,
}

impl TelemetryConfig {
    pub fn from_env() -> Self {
        use std::env;
        Self {
            log_filter: env::var("RUST_LOG")
                .unwrap_or_else(|_| "info".into()),
            log_format: match env::var("ARCANUM_LOG_FORMAT").as_deref() {
                Ok("json") => LogFormat::Json,
                _          => LogFormat::Pretty,
            },
            otlp_endpoint: env::var("OTEL_EXPORTER_OTLP_ENDPOINT").ok(),
            otlp_protocol: match env::var("OTEL_EXPORTER_OTLP_PROTOCOL").as_deref() {
                Ok("http/protobuf") => OtlpProtocol::HttpProtobuf,
                _                   => OtlpProtocol::Grpc,
            },
            service_name: env::var("OTEL_SERVICE_NAME")
                .unwrap_or_else(|_| "arcanum".into()),
            metrics_enabled: env::var("ARCANUM_METRICS_ENABLED")
                .map(|v| !matches!(v.to_lowercase().as_str(), "false" | "0" | "no" | "off"))
                .unwrap_or(true),
            metrics_otlp: env::var("ARCANUM_METRICS_OTLP")
                .map(|v| v == "true")
                .unwrap_or(false),
            metrics_token: env::var("ARCANUM_METRICS_TOKEN").ok(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use serial_test::serial;

    fn clear_telemetry_env() {
        for key in &[
            "RUST_LOG", "ARCANUM_LOG_FORMAT",
            "OTEL_EXPORTER_OTLP_ENDPOINT", "OTEL_EXPORTER_OTLP_PROTOCOL",
            "OTEL_SERVICE_NAME", "ARCANUM_METRICS_ENABLED",
            "ARCANUM_METRICS_OTLP", "ARCANUM_METRICS_TOKEN",
        ] {
            env::remove_var(key);
        }
    }

    #[test]
    #[serial]
    fn defaults_when_no_env_set() {
        clear_telemetry_env();
        let cfg = TelemetryConfig::from_env();
        assert_eq!(cfg.log_filter, "info");
        assert_eq!(cfg.log_format, LogFormat::Pretty);
        assert!(cfg.otlp_endpoint.is_none());
        assert_eq!(cfg.otlp_protocol, OtlpProtocol::Grpc);
        assert_eq!(cfg.service_name, "arcanum");
        assert!(cfg.metrics_enabled);
        assert!(!cfg.metrics_otlp);
        assert!(cfg.metrics_token.is_none());
    }

    #[test]
    #[serial]
    fn otlp_endpoint_is_captured() {
        clear_telemetry_env();
        env::set_var("OTEL_EXPORTER_OTLP_ENDPOINT", "http://localhost:4317");
        let cfg = TelemetryConfig::from_env();
        assert_eq!(cfg.otlp_endpoint.as_deref(), Some("http://localhost:4317"));
    }

    #[test]
    #[serial]
    fn json_log_format_parsed() {
        clear_telemetry_env();
        env::set_var("ARCANUM_LOG_FORMAT", "json");
        let cfg = TelemetryConfig::from_env();
        assert_eq!(cfg.log_format, LogFormat::Json);
    }

    #[test]
    #[serial]
    fn http_protobuf_protocol_parsed() {
        clear_telemetry_env();
        env::set_var("OTEL_EXPORTER_OTLP_PROTOCOL", "http/protobuf");
        let cfg = TelemetryConfig::from_env();
        assert_eq!(cfg.otlp_protocol, OtlpProtocol::HttpProtobuf);
    }

    #[test]
    #[serial]
    fn custom_service_name() {
        clear_telemetry_env();
        env::set_var("OTEL_SERVICE_NAME", "arcanum-prod");
        let cfg = TelemetryConfig::from_env();
        assert_eq!(cfg.service_name, "arcanum-prod");
    }

    #[test]
    #[serial]
    fn metrics_disabled_via_env() {
        clear_telemetry_env();
        env::set_var("ARCANUM_METRICS_ENABLED", "false");
        let cfg = TelemetryConfig::from_env();
        assert!(!cfg.metrics_enabled);
    }

    #[test]
    #[serial]
    fn metrics_otlp_enabled_via_env() {
        clear_telemetry_env();
        env::set_var("ARCANUM_METRICS_OTLP", "true");
        let cfg = TelemetryConfig::from_env();
        assert!(cfg.metrics_otlp);
    }

    #[test]
    #[serial]
    fn metrics_disabled_for_uppercase_false() {
        clear_telemetry_env();
        env::set_var("ARCANUM_METRICS_ENABLED", "FALSE");
        assert!(!TelemetryConfig::from_env().metrics_enabled);
    }

    #[test]
    #[serial]
    fn metrics_disabled_for_zero() {
        clear_telemetry_env();
        env::set_var("ARCANUM_METRICS_ENABLED", "0");
        assert!(!TelemetryConfig::from_env().metrics_enabled);
    }

    #[test]
    #[serial]
    fn metrics_disabled_for_no() {
        clear_telemetry_env();
        env::set_var("ARCANUM_METRICS_ENABLED", "no");
        assert!(!TelemetryConfig::from_env().metrics_enabled);
    }

    #[test]
    #[serial]
    fn metrics_disabled_for_off() {
        clear_telemetry_env();
        env::set_var("ARCANUM_METRICS_ENABLED", "off");
        assert!(!TelemetryConfig::from_env().metrics_enabled);
    }

    #[test]
    #[serial]
    fn metrics_token_captured() {
        clear_telemetry_env();
        env::set_var("ARCANUM_METRICS_TOKEN", "secret123");
        let cfg = TelemetryConfig::from_env();
        assert_eq!(cfg.metrics_token.as_deref(), Some("secret123"));
    }
}
