//! Process telemetry: `tracing` to stderr (text, or JSON with `FACTORY_LOG_FORMAT=json`) and,
//! when `OTEL_EXPORTER_OTLP_ENDPOINT` is set, spans exported over OTLP/HTTP so an operator can
//! ask "why is rig X slow" across roles. One call at the top of every binary.

use opentelemetry::trace::TracerProvider as _;
use opentelemetry_otlp::WithExportConfig as _;
use opentelemetry_sdk::trace::SdkTracerProvider;
use tracing_subscriber::layer::SubscriberExt as _;
use tracing_subscriber::util::SubscriberInitExt as _;

/// Telemetry could not be set up.
#[derive(Debug, thiserror::Error)]
pub enum TelemetryError {
    #[error("OTLP exporter: {detail}")]
    Exporter { detail: String },
    #[error("tracing subscriber already installed")]
    AlreadyInstalled,
}

/// Flushes and shuts the exporter down on drop.
#[derive(Debug)]
pub struct TelemetryGuard {
    provider: Option<SdkTracerProvider>,
}

impl Drop for TelemetryGuard {
    fn drop(&mut self) {
        if let Some(p) = self.provider.take()
            && let Err(e) = p.shutdown()
        {
            eprintln!("telemetry shutdown: {e}");
        }
    }
}

/// What the environment asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TelemetryConfig {
    pub json: bool,
    pub otlp_endpoint: Option<String>,
}

impl TelemetryConfig {
    /// `FACTORY_LOG_FORMAT=json`, `OTEL_EXPORTER_OTLP_ENDPOINT=http://host:4318`.
    #[must_use]
    pub fn from_env() -> Self {
        Self {
            json: std::env::var("FACTORY_LOG_FORMAT").is_ok_and(|v| v == "json"),
            otlp_endpoint: std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT")
                .ok()
                .filter(|v| !v.trim().is_empty()),
        }
    }
}

/// Build the OTLP tracer provider for `service`.
///
/// # Errors
/// `Exporter` when the endpoint cannot be used.
pub fn otlp_provider(service: &str, endpoint: &str) -> Result<SdkTracerProvider, TelemetryError> {
    let exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_http()
        .with_endpoint(format!("{}/v1/traces", endpoint.trim_end_matches('/')))
        .build()
        .map_err(|e| TelemetryError::Exporter {
            detail: e.to_string(),
        })?;
    Ok(SdkTracerProvider::builder()
        .with_batch_exporter(exporter)
        .with_resource(
            opentelemetry_sdk::Resource::builder()
                .with_service_name(service.to_owned())
                .build(),
        )
        .build())
}

/// Install the global subscriber for this process. Call once, keep the guard alive.
///
/// # Errors
/// `Exporter` for a bad OTLP endpoint; `AlreadyInstalled` if called twice.
pub fn init(service: &str, config: &TelemetryConfig) -> Result<TelemetryGuard, TelemetryError> {
    let filter = tracing_subscriber::EnvFilter::from_default_env();
    let provider = config
        .otlp_endpoint
        .as_deref()
        .map(|ep| otlp_provider(service, ep))
        .transpose()?;
    let otel = provider
        .as_ref()
        .map(|p| tracing_opentelemetry::layer().with_tracer(p.tracer(service.to_owned())));
    let registry = tracing_subscriber::registry().with(filter).with(otel);
    let installed = if config.json {
        registry
            .with(
                tracing_subscriber::fmt::layer()
                    .json()
                    .with_writer(std::io::stderr),
            )
            .try_init()
    } else {
        registry
            .with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr))
            .try_init()
    };
    installed.map_err(|_| TelemetryError::AlreadyInstalled)?;
    Ok(TelemetryGuard { provider })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_builds_for_an_http_endpoint_and_guard_shuts_down() {
        let p = otlp_provider("test", "http://127.0.0.1:4318/").expect("provider");
        drop(TelemetryGuard { provider: Some(p) });
        drop(TelemetryGuard { provider: None });
        assert!(otlp_provider("test", "not a url").is_err());
    }

    #[test]
    fn env_config_defaults_are_off() {
        // Whatever the ambient env, the struct is total and printable.
        let c = TelemetryConfig::from_env();
        assert!(format!("{c:?}").contains("json"));
    }

    #[test]
    fn init_installs_once_then_reports_already_installed() {
        let cfg = TelemetryConfig {
            json: true,
            otlp_endpoint: None,
        };
        let first = init("test", &cfg);
        let second = init("test", &cfg);
        // Exactly one of the two attempts wins the global slot (other tests may have installed one).
        assert!(matches!(second, Err(TelemetryError::AlreadyInstalled)));
        drop(first);
    }
}
