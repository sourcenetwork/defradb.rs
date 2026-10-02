//! Logging initialization

use tracing::Level;
use tracing_subscriber::fmt::{self, format::FmtSpan};
use tracing_subscriber::layer::SubscriberExt;
#[cfg(feature = "profiling")]
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Registry};

#[cfg(feature = "profiling")]
use std::fs::File;
#[cfg(feature = "profiling")]
use std::path::PathBuf;
#[cfg(feature = "profiling")]
use std::time::{SystemTime, UNIX_EPOCH};
#[cfg(feature = "profiling")]
use tracing_chrome::{ChromeLayerBuilder, FlushGuard};

use crate::config::{Config, LogFormat, LogLevel, LogOutput};
use crate::error::{Error, Result};

/// The event bridge only earns its keep when spans are exported; without
/// `otel` the reporter discards them, so attaching events would be pure cost.
#[cfg(feature = "otel")]
fn fastrace_event_layer() -> Option<telemetry::FastraceEventLayer> {
    Some(telemetry::FastraceEventLayer)
}

#[cfg(not(feature = "otel"))]
fn fastrace_event_layer() -> Option<tracing_subscriber::layer::Identity> {
    None
}

/// Captures dependency instrumentation. `iroh`, `libp2p`, `quinn` and
/// `hickory` emit `tracing` spans, which `fastrace` cannot see; this mirrors
/// them, rooting any that arrive without a local parent so background tasks
/// are not dropped. The subscriber-wide `EnvFilter` runs first, so the cost is
/// bounded by the configured level rather than by how chatty a dependency is.
#[cfg(feature = "otel")]
fn fastrace_compat_layer<S>() -> Option<telemetry::FastraceCompatLayer<S>>
where
    S: tracing::Subscriber + for<'a> tracing_subscriber::registry::LookupSpan<'a>,
{
    Some(telemetry::FastraceCompatLayer::new())
}

#[cfg(not(feature = "otel"))]
fn fastrace_compat_layer() -> Option<tracing_subscriber::layer::Identity> {
    None
}

fn with_default_transport_noise_filters(filter: EnvFilter) -> EnvFilter {
    filter
        .add_directive(
            "iroh_quinn_proto::connection=error"
                .parse()
                .expect("valid tracing directive"),
        )
        .add_directive(
            "noq_proto::connection=error"
                .parse()
                .expect("valid tracing directive"),
        )
}

pub struct LoggingHandle {
    #[cfg(feature = "profiling")]
    profiling: Option<ProfilingTrace>,
    telemetry: telemetry::TelemetryHandle,
}

#[cfg(feature = "profiling")]
struct ProfilingTrace {
    path: PathBuf,
    guard: FlushGuard,
}

impl LoggingHandle {
    fn new(telemetry: telemetry::TelemetryHandle) -> Self {
        Self {
            #[cfg(feature = "profiling")]
            profiling: None,
            telemetry,
        }
    }

    #[cfg(feature = "profiling")]
    fn with_profile(profiling: ProfilingTrace, telemetry: telemetry::TelemetryHandle) -> Self {
        Self {
            profiling: Some(profiling),
            telemetry,
        }
    }

    pub fn finish(self) {
        #[cfg(feature = "profiling")]
        if let Some(profiling) = self.profiling {
            let path = profiling.path;
            drop(profiling.guard);
            eprintln!("Chrome trace written to {}", path.display());
        }
        // Must precede telemetry shutdown: flush hands buffered spans to the
        // reporter, which exports through the OTLP exporter owned below.
        fastrace::flush();
        self.telemetry.shutdown();
    }
}

/// Initialize logging based on configuration.
///
/// `enable_telemetry` gates the OTLP exporter independently of logging: only
/// the `start` command sets it, matching Go (which configures telemetry only
/// in `cli/start.go`). Ephemeral commands like `version` / `client` keep the
/// fmt subscriber but never spin up the exporter thread or pay its shutdown
/// cost.
/// Spans are produced by `fastrace`. When nothing consumes them — no `otel`
/// feature, or telemetry disabled at runtime — install a reporter that drops
/// each batch. This is the analogue of the old `tracing` spans reaching the
/// Registry and being discarded under `FmtSpan::NONE`.
struct DropReporter;

impl fastrace::collector::Reporter for DropReporter {
    fn report(&mut self, _spans: Vec<fastrace::prelude::SpanRecord>) {}
}

pub fn init(
    config: &Config,
    enable_profiling: bool,
    enable_telemetry: bool,
) -> Result<LoggingHandle> {
    let level = match config.log.level {
        LogLevel::Debug => Level::DEBUG,
        LogLevel::Info => Level::INFO,
        LogLevel::Error => Level::ERROR,
        LogLevel::Fatal => Level::ERROR,
    };

    let filter = with_default_transport_noise_filters(
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(level.to_string())),
    );

    // Span callsites read the same directives as events, so `RUST_LOG` (or the
    // configured level) governs both and there is only one filter language to
    // learn. `fastrace` has no filtering of its own, so without this a span
    // left in a hot path could only be switched off by recompiling.
    //
    // The transport-noise directives above are deliberately not included: they
    // scope third-party event targets, and those crates emit no `defra_trace`
    // spans, so they would never match a span callsite.
    defra_trace::set_directives(&std::env::var("RUST_LOG").unwrap_or_else(|_| level.to_string()));

    let builder = fmt::layer()
        .with_target(true)
        .with_thread_ids(false)
        .with_thread_names(false)
        .with_file(config.log.source)
        .with_line_number(config.log.source)
        .with_ansi(!config.log.color_disabled);

    let builder = if config.log.stacktrace {
        builder.with_span_events(FmtSpan::CLOSE)
    } else {
        builder.with_span_events(FmtSpan::NONE)
    };

    match (config.log.format, config.log.output) {
        (LogFormat::Json, LogOutput::Stdout) => init_subscriber(
            filter,
            builder.json().with_writer(std::io::stdout),
            enable_profiling,
            enable_telemetry,
            config,
        ),
        (LogFormat::Json, LogOutput::Stderr) => init_subscriber(
            filter,
            builder.json().with_writer(std::io::stderr),
            enable_profiling,
            enable_telemetry,
            config,
        ),
        (LogFormat::Text, LogOutput::Stdout) => init_subscriber(
            filter,
            builder.with_writer(std::io::stdout),
            enable_profiling,
            enable_telemetry,
            config,
        ),
        (LogFormat::Text, LogOutput::Stderr) => init_subscriber(
            filter,
            builder.with_writer(std::io::stderr),
            enable_profiling,
            enable_telemetry,
            config,
        ),
    }
}

fn init_subscriber<L>(
    filter: EnvFilter,
    fmt_layer: L,
    enable_profiling: bool,
    enable_telemetry: bool,
    config: &Config,
) -> Result<LoggingHandle>
where
    L: tracing_subscriber::Layer<Registry> + Send + Sync + 'static,
{
    #[cfg(not(feature = "profiling"))]
    if enable_profiling {
        return Err(Error::LoggingInit(
            "profiling support requires building the CLI with `--features profiling`".into(),
        ));
    }

    // Telemetry init. Mirrors Go's default-on behavior: when compiled with
    // `--features otel`, the exporter is active for the `start` command
    // (enable_telemetry) unless `--no-telemetry` / `DEFRA_NO_TELEMETRY` /
    // `telemetry_disabled` opts out. If exporter construction fails (e.g.
    // malformed env var), we log and continue with no telemetry — matches
    // Go's `log.ErrorContextE` + continue path.
    #[cfg(feature = "otel")]
    let telemetry_handle = if config.telemetry_disabled || !enable_telemetry {
        fastrace::set_reporter(DropReporter, fastrace::collector::Config::default());
        telemetry::TelemetryHandle::noop()
    } else {
        // Source a Go-style descriptive build string for service.version
        // (`defradb <ver> (<commit8> <date>) built with ...`) so OTLP
        // receivers grouping on service.version see the same shape Go emits,
        // not the telemetry crate's bare semver.
        let telemetry_config = telemetry::TelemetryConfig::new(
            "DefraDB",
            defra_version::VersionInfo::new().descriptive(),
        );
        match telemetry::init(telemetry_config) {
            Ok((handle, reporter)) => {
                fastrace::set_reporter(reporter, fastrace::collector::Config::default());
                handle
            }
            Err(err) => {
                eprintln!(
                    "warning: failed to configure OpenTelemetry, continuing without telemetry: {err}"
                );
                telemetry::TelemetryHandle::noop()
            }
        }
    };

    #[cfg(not(feature = "otel"))]
    let telemetry_handle = {
        let _ = (config, enable_telemetry); // unused when otel is off
        fastrace::set_reporter(DropReporter, fastrace::collector::Config::default());
        telemetry::TelemetryHandle::noop()
    };

    // Suppress exporter-unreachable spam on every layer that ingests OTel
    // events. The filter is stateless and the one-shot operator hint it
    // emits is guarded by a process-global latch, so attaching a fresh
    // instance to each layer is fine — the hint still appears exactly once
    // across all of them.
    #[cfg(feature = "otel")]
    let fmt_layer = fmt_layer.with_filter(telemetry::OtelDedupFilter::new());

    #[cfg(feature = "profiling")]
    if enable_profiling {
        let (chrome_layer, profiling) = build_chrome_layer()?;
        #[cfg(feature = "otel")]
        let chrome_layer = chrome_layer.with_filter(telemetry::OtelDedupFilter::new());
        let registry = tracing_subscriber::registry()
            .with(fmt_layer)
            .with(chrome_layer)
            .with(fastrace_event_layer())
            .with(fastrace_compat_layer());
        registry
            .with(filter)
            .try_init()
            .map_err(|error| Error::LoggingInit(error.to_string()))?;
        return Ok(LoggingHandle::with_profile(profiling, telemetry_handle));
    }

    let registry = tracing_subscriber::registry()
        .with(fmt_layer)
        .with(fastrace_event_layer())
        .with(fastrace_compat_layer());
    registry
        .with(filter)
        .try_init()
        .map_err(|error| Error::LoggingInit(error.to_string()))?;

    Ok(LoggingHandle::new(telemetry_handle))
}

#[cfg(feature = "profiling")]
fn build_chrome_layer<S>() -> Result<(tracing_chrome::ChromeLayer<S>, ProfilingTrace)>
where
    S: tracing::Subscriber + for<'span> LookupSpan<'span> + Send + Sync,
{
    let path = trace_output_path()?;
    let file = File::create(&path).map_err(|error| {
        Error::LoggingInit(format!(
            "failed to create profiling trace file {}: {}",
            path.display(),
            error
        ))
    })?;
    let (layer, guard) = ChromeLayerBuilder::new()
        .writer(file)
        .include_args(true)
        .build();

    Ok((layer, ProfilingTrace { path, guard }))
}

#[cfg(feature = "profiling")]
fn trace_output_path() -> Result<PathBuf> {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| Error::LoggingInit(format!("failed to read system time: {}", error)))?
        .as_millis();

    Ok(std::env::current_dir()
        .map_err(Error::Io)?
        .join(format!("defra-trace-{}.json", timestamp)))
}
