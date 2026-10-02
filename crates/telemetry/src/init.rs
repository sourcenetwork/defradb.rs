//! OTLP exporter setup. Mirrors Go DefraDB's `internal/telemetry/otel.go`.
//!
//! - OTLP/HTTP transport (`reqwest-blocking-client`). Same wire protocol Go
//!   uses via `otlptracehttp`. Default endpoint is `http://localhost:4318`.
//! - Standard OTEL env vars are honored automatically by `opentelemetry-otlp`
//!   0.32: `OTEL_EXPORTER_OTLP_ENDPOINT`, `OTEL_EXPORTER_OTLP_HEADERS`,
//!   `OTEL_EXPORTER_OTLP_PROTOCOL`, signal-specific overrides, etc.
//! - Resource attributes: `service.name`, `service.version`, `os.type`,
//!   `host.arch`, `process.pid`, `process.executable.name`. Mirrors Go's
//!   `resource.WithOS()` + `resource.WithProcess()` (which we approximate
//!   with `std::env::consts` + `std::process` to avoid an extra dep).
//!
use std::borrow::Cow;

use fastrace_opentelemetry::OpenTelemetryReporter;
use opentelemetry::global;
use opentelemetry::InstrumentationScope;
use opentelemetry::KeyValue;
use opentelemetry_otlp::{MetricExporter, SpanExporter, WithHttpConfig};
use opentelemetry_sdk::metrics::SdkMeterProvider;
use opentelemetry_sdk::Resource;
use thiserror::Error;

use crate::config::TelemetryConfig;
use crate::handle::TelemetryHandle;
use crate::util::{otel_timeout, panic_message};

#[derive(Debug, Error)]
pub enum InitError {
    #[error("failed to build OTLP span exporter: {0}")]
    SpanExporter(#[source] Box<dyn std::error::Error + Send + Sync>),

    #[error("failed to build OTLP metric exporter: {0}")]
    MetricExporter(#[source] Box<dyn std::error::Error + Send + Sync>),

    #[error("failed to spawn the telemetry HTTP client init thread: {0}")]
    HttpClientThreadSpawn(#[source] std::io::Error),

    #[error("the telemetry HTTP client init thread panicked: {0}")]
    HttpClientThreadPanic(String),

    #[error("failed to build the telemetry HTTP client: {0}")]
    HttpClientBuild(#[source] reqwest::Error),
}

/// Returns the lifecycle handle and a [`OpenTelemetryReporter`] for
/// `fastrace::set_reporter`. Spans are produced by `fastrace`, so the OTLP
/// span path is the reporter rather than a `tracing` layer; the exporter and
/// resource below are the same ones Go's `otlptracehttp` setup uses.
///
/// The reporter drives export with `pollster::block_on`, which is correct for
/// the `reqwest-blocking-client` transport selected in this crate's
/// `Cargo.toml` — no Tokio runtime is required. Batching is `fastrace`'s own
/// collector interval rather than `BatchSpanProcessor`.
///
/// Safe to call from any context (no Tokio runtime required): with the
/// `reqwest-blocking-client` transport selected in this crate's `Cargo.toml`,
/// `BatchSpanProcessor` spawns a dedicated OS thread via `std::thread`. No
/// `Handle::current()` call happens at init.
///
/// By default `init` installs the providers in the process-wide
/// `opentelemetry::global` slot. Set [`TelemetryConfig::install_global`] to
/// `false` (or call [`TelemetryConfig::without_global`]) to skip that —
/// useful when the host process already runs its own OTel stack and would
/// otherwise see its globals silently replaced.
pub fn init(
    config: TelemetryConfig,
) -> Result<(TelemetryHandle, OpenTelemetryReporter), InitError> {
    let executable_name = std::env::current_exe()
        .ok()
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
        .unwrap_or_default();
    let resource = Resource::builder()
        .with_schema_url(
            // Declare which semantic-conventions version these attributes
            // follow, matching Go's `resource.WithSchemaURL(semconv.SchemaURL)`.
            // The attributes themselves are passed via `.with_*` below, so the
            // attribute iterator here is empty.
            std::iter::empty::<KeyValue>(),
            opentelemetry_semantic_conventions::SCHEMA_URL,
        )
        .with_service_name(config.service_name.clone())
        .with_attribute(KeyValue::new("service.version", config.service_version))
        .with_attribute(KeyValue::new("os.type", std::env::consts::OS))
        .with_attribute(KeyValue::new("host.arch", std::env::consts::ARCH))
        .with_attribute(KeyValue::new("process.pid", i64::from(std::process::id())))
        .with_attribute(KeyValue::new("process.executable.name", executable_name))
        .build();

    let http_client = std::thread::Builder::new()
        .name("telemetry-http-client-init".into())
        .spawn(move || {
            reqwest::blocking::Client::builder()
                .timeout(otel_timeout())
                .build()
        })
        .map_err(InitError::HttpClientThreadSpawn)?
        .join()
        .map_err(|payload| InitError::HttpClientThreadPanic(panic_message(&payload)))?
        .map_err(InitError::HttpClientBuild)?;

    let span_exporter = SpanExporter::builder()
        .with_http()
        .with_http_client(http_client.clone())
        .build()
        .map_err(|e| InitError::SpanExporter(Box::new(e)))?;

    let reporter = OpenTelemetryReporter::new(
        span_exporter,
        Cow::Owned(resource.clone()),
        InstrumentationScope::builder(config.service_name.clone()).build(),
    );

    let metric_exporter = MetricExporter::builder()
        .with_http()
        .with_http_client(http_client.clone())
        .build()
        .map_err(|e| InitError::MetricExporter(Box::new(e)))?;

    let meter_provider = SdkMeterProvider::builder()
        .with_periodic_exporter(metric_exporter)
        .with_resource(resource)
        .build();
    let metric_installation = crate::metrics::install(&meter_provider);

    if config.install_global {
        global::set_meter_provider(meter_provider.clone());
    }

    let handle = TelemetryHandle {
        meter_provider: Some(meter_provider),
        metric_installation: Some(metric_installation),
    };

    Ok((handle, reporter))
}
pub use fastrace_opentelemetry::OpenTelemetryReporter as Reporter;
