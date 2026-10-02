//! OpenTelemetry exporter setup for DefraDB.
//!
//! Mirrors Go DefraDB's `internal/telemetry/otel.go`. The `otlp` feature
//! compiles in OTLP/HTTP trace and metric export together with
//! `OtelDedupFilter`; without it the crate is the no-op [`TelemetryHandle`] and
//! the conflict/retry counters alone, and carries no `tracing-subscriber`
//! dependency.
//!
//! Connection-refused log spam from the OTEL SDK is deduped via
//! `OtelDedupFilter` — the Rust equivalent of Go's `sync.Once`-guarded
//! `otel.SetErrorHandler`. The global error handler API was removed from
//! `opentelemetry::global` in v0.27; SDK diagnostics now flow through
//! `tracing` at `target = "opentelemetry"*`, so a `tracing_subscriber`
//! filter is the natural hook.

mod config;
#[cfg(feature = "otlp")]
mod dedup;
#[cfg(feature = "otlp")]
mod events;
mod handle;
mod metrics;
#[cfg(feature = "otlp")]
mod util;

pub use config::TelemetryConfig;
#[cfg(feature = "otlp")]
pub use dedup::OtelDedupFilter;
#[cfg(feature = "otlp")]
pub use events::FastraceEventLayer;
/// Mirrors spans and events from crates instrumented with `tracing` — our
/// dependencies — into `fastrace`. It roots a span that has no local parent,
/// so background tasks in `libp2p` and `iroh` are captured rather than
/// dropped. Complements [`FastraceEventLayer`], which handles events emitted
/// inside our own `fastrace` spans; this one handles events inside third-party
/// `tracing` spans.
#[cfg(feature = "otlp")]
pub use fastrace_tracing::FastraceCompatLayer;
pub use handle::TelemetryHandle;
pub use metrics::{
    conflict_metrics_snapshot, record_commit_gate_wait, record_conflict_tracker_size,
    record_escaped_conflict, record_retry_attempt, record_retry_exhaustion, record_retry_success,
    record_storage_conflict, ConflictMetricsSnapshot, RetryLayer, RetryLayerSnapshot,
};

#[cfg(feature = "otlp")]
mod init;
#[cfg(feature = "otlp")]
pub use init::{init, InitError, Reporter};
