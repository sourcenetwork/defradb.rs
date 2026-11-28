use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Weak;

#[cfg(feature = "otlp")]
use std::sync::Arc;

#[cfg(feature = "otlp")]
use kovan::Atom;
#[cfg(feature = "otlp")]
use opentelemetry::metrics::{Counter, MeterProvider as _, ObservableCounter, ObservableGauge};
#[cfg(feature = "otlp")]
use opentelemetry::KeyValue;
#[cfg(feature = "otlp")]
use regolith::sync::OnceCell;

/// A retry loop that absorbs transaction conflicts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(usize)]
pub enum RetryLayer {
    Engine,
    PushMarker,
}

impl RetryLayer {
    const COUNT: usize = 2;

    #[cfg(feature = "otlp")]
    const fn labels(self) -> &'static [&'static str] {
        match self {
            Self::Engine => &["engine"],
            // Preserve the pre-stage-3 label for one compatibility window
            // while exposing the marker-based name to new dashboards.
            Self::PushMarker => &["push_marker", "push_ledger"],
        }
    }
}

/// Process-lifetime retry counters for one layer.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RetryLayerSnapshot {
    /// Retries performed after an initial conflict.
    pub attempts: u64,
    /// Retry sequences whose final result was no longer a conflict.
    pub successes: u64,
    /// Retry sequences that returned a conflict after reaching their bound.
    pub exhaustions: u64,
}

/// Process-lifetime conflict retry and escape counters.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ConflictMetricsSnapshot {
    /// Retired HTTP retry layer, preserved as zero for historical readers.
    pub http_auto_commit: RetryLayerSnapshot,
    /// Retired embedded retry layer, preserved as zero for historical readers.
    pub embedded_execute: RetryLayerSnapshot,
    /// Retired merge retry layer, preserved as zero for historical readers.
    pub merge: RetryLayerSnapshot,
    pub push_marker: RetryLayerSnapshot,
    /// Transaction attempts re-executed after a prior attempt lost a conflict.
    /// Only `attempts` is recorded for this layer; completion and exhaustion
    /// are owned by the retrying caller.
    pub engine: EngineRetrySnapshot,
    /// Retired ID-reservation retry layer, preserved as zero for historical readers.
    pub doc_short_id_reservation: RetryLayerSnapshot,
    /// Typed transaction conflicts returned through a client API.
    pub escaped_to_clients: u64,
}

/// Process-lifetime count of transaction-engine replay executions.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EngineRetrySnapshot {
    pub attempts: u64,
}

struct RetryCounters {
    attempts: [AtomicU64; RetryLayer::COUNT],
    successes: [AtomicU64; RetryLayer::COUNT],
    exhaustions: [AtomicU64; RetryLayer::COUNT],
    escaped_to_clients: AtomicU64,
}

impl RetryCounters {
    const fn new() -> Self {
        Self {
            attempts: [const { AtomicU64::new(0) }; RetryLayer::COUNT],
            successes: [const { AtomicU64::new(0) }; RetryLayer::COUNT],
            exhaustions: [const { AtomicU64::new(0) }; RetryLayer::COUNT],
            escaped_to_clients: AtomicU64::new(0),
        }
    }

    fn layer(&self, layer: RetryLayer) -> RetryLayerSnapshot {
        let index = layer as usize;
        RetryLayerSnapshot {
            attempts: self.attempts[index].load(Ordering::Relaxed),
            successes: self.successes[index].load(Ordering::Relaxed),
            exhaustions: self.exhaustions[index].load(Ordering::Relaxed),
        }
    }
}

static RETRIES: RetryCounters = RetryCounters::new();

/// Capture process-lifetime conflict retry and escape counters.
pub fn conflict_metrics_snapshot() -> ConflictMetricsSnapshot {
    ConflictMetricsSnapshot {
        http_auto_commit: RetryLayerSnapshot::default(),
        embedded_execute: RetryLayerSnapshot::default(),
        merge: RetryLayerSnapshot::default(),
        push_marker: RETRIES.layer(RetryLayer::PushMarker),
        engine: EngineRetrySnapshot {
            attempts: RETRIES.attempts[RetryLayer::Engine as usize].load(Ordering::Relaxed),
        },
        doc_short_id_reservation: RetryLayerSnapshot::default(),
        escaped_to_clients: RETRIES.escaped_to_clients.load(Ordering::Relaxed),
    }
}

pub fn record_retry_attempt(layer: RetryLayer) {
    RETRIES.attempts[layer as usize].fetch_add(1, Ordering::Relaxed);
    emit_retry_attempt(layer);
}

pub fn record_retry_success(layer: RetryLayer) {
    RETRIES.successes[layer as usize].fetch_add(1, Ordering::Relaxed);
    emit_retry_success(layer);
}

pub fn record_retry_exhaustion(layer: RetryLayer) {
    RETRIES.exhaustions[layer as usize].fetch_add(1, Ordering::Relaxed);
    emit_retry_exhaustion(layer);
}

pub fn record_escaped_conflict(surface: &'static str) {
    RETRIES.escaped_to_clients.fetch_add(1, Ordering::Relaxed);
    emit_escaped_conflict(surface);
}

/// Count one engine conflict by its bounded access/write categories.
/// Keys and document identities are deliberately not metric labels.
pub fn record_storage_conflict(backend: &'static str, access: &'static str, write: &'static str) {
    #[cfg(feature = "otlp")]
    with_instruments(|metrics| {
        metrics.storage_conflicts.add(
            1,
            &[
                KeyValue::new("backend", backend),
                KeyValue::new("access", access),
                KeyValue::new("write", write),
            ],
        );
    });
    #[cfg(not(feature = "otlp"))]
    let _ = (backend, access, write);
}

static STORAGE_BACKGROUND_ERRORS: AtomicU64 = AtomicU64::new(0);

/// Record a failed background storage job (flush, compaction, manifest
/// or WAL write). The engine keeps running after one, so without this a
/// job that fails on every retry is visible only in the log.
pub fn record_storage_background_error(backend: &'static str, reason: &'static str) {
    STORAGE_BACKGROUND_ERRORS.fetch_add(1, Ordering::Relaxed);
    emit_storage_background_error(backend, reason);
}

/// Failed background storage jobs recorded in this process.
pub fn storage_background_error_count() -> u64 {
    STORAGE_BACKGROUND_ERRORS.load(Ordering::Relaxed)
}

/// A vector memory budget's current values, for telemetry.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct VectorMemoryBudgetSnapshot {
    /// Bytes the vector caches hold now.
    pub used: u64,
    /// Bytes they may hold.
    pub limit: u64,
    /// Charge attempts refused because the budget was full.
    pub refusals: u64,
}

/// A source of one node's vector memory budget values.
pub trait VectorMemoryBudgetSource: Send + Sync {
    fn vector_memory_budget_snapshot(&self) -> VectorMemoryBudgetSnapshot;
}

#[cfg(feature = "otlp")]
static NEXT_VECTOR_BUDGET: AtomicU64 = AtomicU64::new(1);
#[cfg(feature = "otlp")]
type VectorBudgetRegistry = Atom<Vec<(u64, Weak<dyn VectorMemoryBudgetSource>)>>;
#[cfg(feature = "otlp")]
static VECTOR_BUDGETS: OnceCell<VectorBudgetRegistry> = OnceCell::new();

/// Registers a vector memory budget for observable telemetry gauges.
pub fn register_vector_memory_budget(source: Weak<dyn VectorMemoryBudgetSource>) -> u64 {
    #[cfg(feature = "otlp")]
    {
        let id = NEXT_VECTOR_BUDGET.fetch_add(1, Ordering::Relaxed);
        let budgets = VECTOR_BUDGETS.get_or_init_racy(|| Atom::new(Vec::new()));
        budgets.rcu(|current| {
            let mut next: Vec<_> = current
                .iter()
                .filter(|(_, source)| source.strong_count() > 0)
                .cloned()
                .collect();
            next.push((id, source.clone()));
            next
        });
        id
    }
    #[cfg(not(feature = "otlp"))]
    {
        let _ = source;
        0
    }
}

#[cfg(feature = "otlp")]
fn vector_memory_budget_snapshots() -> Vec<(u64, VectorMemoryBudgetSnapshot)> {
    let Some(budgets) = VECTOR_BUDGETS.get() else {
        return Vec::new();
    };
    budgets
        .load()
        .iter()
        .filter_map(|(id, source)| {
            source
                .upgrade()
                .map(|source| (*id, source.vector_memory_budget_snapshot()))
        })
        .collect()
}

/// Something the storage engine, or the store around it, reports: what the
/// engine did in the background, how each transaction ended, and post-commit
/// work that failed after its commit was durable (the commit stands).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(usize)]
pub enum StorageEvent {
    Flush,
    CompactionBegin,
    Compaction,
    TableFileCreated,
    TableFileDeleted,
    Ingest,
    WalTailDiscarded,
    Commit,
    AbortRollback,
    AbortConflict,
    AbortError,
    AbortClosed,
    AbortCallbackPanicked,
    PostCommitHookFailed,
    PostCommitHookPanicked,
    PostCommitHookCancelled,
    PostCommitEventPanicked,
    PostCommitCallbackPanicked,
}

impl StorageEvent {
    const COUNT: usize = 18;

    /// The `event` label of `defradb.storage.events`.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Flush => "flush",
            Self::CompactionBegin => "compaction_begin",
            Self::Compaction => "compaction",
            Self::TableFileCreated => "table_file_created",
            Self::TableFileDeleted => "table_file_deleted",
            Self::Ingest => "ingest",
            Self::WalTailDiscarded => "wal_tail_discarded",
            Self::Commit => "commit",
            Self::AbortRollback => "abort_rollback",
            Self::AbortConflict => "abort_conflict",
            Self::AbortError => "abort_error",
            Self::AbortClosed => "abort_closed",
            Self::AbortCallbackPanicked => "abort_callback_panicked",
            Self::PostCommitHookFailed => "post_commit_hook_failed",
            Self::PostCommitHookPanicked => "post_commit_hook_panicked",
            Self::PostCommitHookCancelled => "post_commit_hook_cancelled",
            Self::PostCommitEventPanicked => "post_commit_event_panicked",
            Self::PostCommitCallbackPanicked => "post_commit_callback_panicked",
        }
    }
}

static STORAGE_EVENTS: [AtomicU64; StorageEvent::COUNT] =
    [const { AtomicU64::new(0) }; StorageEvent::COUNT];

/// Count one storage event, with the bytes it moved when it moved any.
pub fn record_storage_event(event: StorageEvent, bytes: u64) {
    STORAGE_EVENTS[event as usize].fetch_add(1, Ordering::Relaxed);
    emit_storage_event(event, bytes);
}

/// Storage events of this kind recorded in this process.
pub fn storage_event_count(event: StorageEvent) -> u64 {
    STORAGE_EVENTS[event as usize].load(Ordering::Relaxed)
}

#[cfg(feature = "otlp")]
fn emit_storage_event(event: StorageEvent, bytes: u64) {
    with_instruments(|metrics| {
        let labels = [KeyValue::new("event", event.label())];
        metrics.storage_events.add(1, &labels);
        if bytes > 0 {
            metrics.storage_event_bytes.add(bytes, &labels);
        }
    });
}

#[cfg(not(feature = "otlp"))]
fn emit_storage_event(_event: StorageEvent, _bytes: u64) {}

#[cfg(feature = "otlp")]
fn emit_retry_attempt(layer: RetryLayer) {
    with_instruments(|metrics| {
        for label in layer.labels() {
            metrics
                .retry_attempts
                .add(1, &[KeyValue::new("layer", *label)]);
        }
    });
}

#[cfg(feature = "otlp")]
fn emit_retry_success(layer: RetryLayer) {
    with_instruments(|metrics| {
        for label in layer.labels() {
            metrics
                .retry_successes
                .add(1, &[KeyValue::new("layer", *label)]);
        }
    });
}

#[cfg(feature = "otlp")]
fn emit_retry_exhaustion(layer: RetryLayer) {
    with_instruments(|metrics| {
        for label in layer.labels() {
            metrics
                .retry_exhaustions
                .add(1, &[KeyValue::new("layer", *label)]);
        }
    });
}

#[cfg(feature = "otlp")]
fn emit_escaped_conflict(surface: &'static str) {
    with_instruments(|metrics| {
        metrics
            .escaped_conflicts
            .add(1, &[KeyValue::new("surface", surface)]);
    });
}

#[cfg(feature = "otlp")]
fn emit_storage_background_error(backend: &'static str, reason: &'static str) {
    with_instruments(|metrics| {
        metrics.storage_background_errors.add(
            1,
            &[
                KeyValue::new("backend", backend),
                KeyValue::new("reason", reason),
            ],
        );
    });
}

#[cfg(not(feature = "otlp"))]
fn emit_retry_attempt(_layer: RetryLayer) {}
#[cfg(not(feature = "otlp"))]
fn emit_retry_success(_layer: RetryLayer) {}
#[cfg(not(feature = "otlp"))]
fn emit_retry_exhaustion(_layer: RetryLayer) {}
#[cfg(not(feature = "otlp"))]
fn emit_escaped_conflict(_surface: &'static str) {}
#[cfg(not(feature = "otlp"))]
fn emit_storage_background_error(_backend: &'static str, _reason: &'static str) {}

#[cfg(feature = "otlp")]
struct Instruments {
    storage_conflicts: Counter<u64>,
    storage_background_errors: Counter<u64>,
    storage_events: Counter<u64>,
    storage_event_bytes: Counter<u64>,
    retry_attempts: Counter<u64>,
    retry_successes: Counter<u64>,
    retry_exhaustions: Counter<u64>,
    escaped_conflicts: Counter<u64>,
    _vector_memory_used: ObservableGauge<u64>,
    _vector_memory_limit: ObservableGauge<u64>,
    _vector_memory_refusals: ObservableCounter<u64>,
}

#[cfg(feature = "otlp")]
static NEXT_INSTALLATION: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "otlp")]
type InstrumentRegistry = Atom<Vec<(u64, Arc<Instruments>)>>;
#[cfg(feature = "otlp")]
static INSTRUMENTS: OnceCell<InstrumentRegistry> = OnceCell::new();

#[cfg(feature = "otlp")]
pub(crate) fn install(provider: &opentelemetry_sdk::metrics::SdkMeterProvider) -> u64 {
    let meter = provider.meter("defradb");
    let vector_memory_used = meter
        .u64_observable_gauge("defradb.vector.memory.used")
        .with_description("Bytes held by vector index caches")
        .with_callback(|observer| {
            for (budget, snapshot) in vector_memory_budget_snapshots() {
                let labels = [KeyValue::new("budget", budget.to_string())];
                observer.observe(snapshot.used, &labels);
            }
        })
        .build();
    let vector_memory_limit = meter
        .u64_observable_gauge("defradb.vector.memory.limit")
        .with_description("Bytes vector index caches may hold")
        .with_callback(|observer| {
            for (budget, snapshot) in vector_memory_budget_snapshots() {
                let labels = [KeyValue::new("budget", budget.to_string())];
                observer.observe(snapshot.limit, &labels);
            }
        })
        .build();
    let vector_memory_refusals = meter
        .u64_observable_counter("defradb.vector.memory.refusals")
        .with_description("Vector cache charge attempts refused by the memory budget")
        .with_callback(|observer| {
            for (budget, snapshot) in vector_memory_budget_snapshots() {
                let labels = [KeyValue::new("budget", budget.to_string())];
                observer.observe(snapshot.refusals, &labels);
            }
        })
        .build();
    let instruments = Arc::new(Instruments {
        storage_conflicts: meter
            .u64_counter("defradb.storage.conflicts")
            .with_description("Engine commit conflicts by losing access and winning write")
            .build(),
        storage_background_errors: meter
            .u64_counter("defradb.storage.background.errors")
            .with_description("Failed background storage jobs")
            .build(),
        storage_events: meter
            .u64_counter("defradb.storage.events")
            .with_description("Storage engine and transaction events by kind")
            .build(),
        storage_event_bytes: meter
            .u64_counter("defradb.storage.event.bytes")
            .with_description("Bytes moved by storage events, by kind")
            .build(),
        retry_attempts: meter
            .u64_counter("defradb.transaction.retry.attempts")
            .with_description("Transaction conflict retry attempts")
            .build(),
        retry_successes: meter
            .u64_counter("defradb.transaction.retry.successes")
            .with_description("Conflicts recovered by a retry layer")
            .build(),
        retry_exhaustions: meter
            .u64_counter("defradb.transaction.retry.exhaustions")
            .with_description("Transaction conflict retry loops exhausted")
            .build(),
        escaped_conflicts: meter
            .u64_counter("defradb.transaction.conflicts.escaped")
            .with_description("Transaction conflicts returned to clients")
            .build(),
        _vector_memory_used: vector_memory_used,
        _vector_memory_limit: vector_memory_limit,
        _vector_memory_refusals: vector_memory_refusals,
    });

    crate::engine_tickers::register(&meter);

    let installation = NEXT_INSTALLATION.fetch_add(1, Ordering::Relaxed) + 1;
    let registry = INSTRUMENTS.get_or_init_racy(|| Atom::new(Vec::new()));
    registry.rcu(|current| {
        let mut next = current.clone();
        next.push((installation, Arc::clone(&instruments)));
        next
    });
    installation
}

#[cfg(feature = "otlp")]
pub(crate) fn uninstall(installation: u64) {
    if let Some(registry) = INSTRUMENTS.get() {
        registry.rcu(|current| {
            current
                .iter()
                .filter(|(id, _)| *id != installation)
                .cloned()
                .collect()
        });
    }
}

#[cfg(feature = "otlp")]
fn with_instruments(record: impl FnOnce(&Instruments)) {
    let Some(registry) = INSTRUMENTS.get() else {
        return;
    };
    let guard = registry.load();
    if let Some((_, metrics)) = guard.last() {
        record(metrics);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_storage_event_has_its_own_label_and_counter() {
        let events = [
            StorageEvent::Flush,
            StorageEvent::CompactionBegin,
            StorageEvent::Compaction,
            StorageEvent::TableFileCreated,
            StorageEvent::TableFileDeleted,
            StorageEvent::Ingest,
            StorageEvent::WalTailDiscarded,
            StorageEvent::Commit,
            StorageEvent::AbortRollback,
            StorageEvent::AbortConflict,
            StorageEvent::AbortError,
            StorageEvent::AbortClosed,
            StorageEvent::AbortCallbackPanicked,
            StorageEvent::PostCommitHookFailed,
            StorageEvent::PostCommitHookPanicked,
            StorageEvent::PostCommitHookCancelled,
            StorageEvent::PostCommitEventPanicked,
            StorageEvent::PostCommitCallbackPanicked,
        ];
        assert_eq!(events.len(), StorageEvent::COUNT);
        let labels: std::collections::BTreeSet<_> = events.iter().map(|e| e.label()).collect();
        assert_eq!(labels.len(), StorageEvent::COUNT);
        for (index, event) in events.into_iter().enumerate() {
            assert_eq!(event as usize, index);
            let before = storage_event_count(event);
            record_storage_event(event, 3);
            assert!(storage_event_count(event) > before);
        }
    }

    #[cfg(feature = "otlp")]
    #[test]
    fn vector_budget_metrics_report_the_registered_values_and_prune_dead_budgets() {
        use opentelemetry_sdk::metrics::data::{AggregatedMetrics, MetricData, ResourceMetrics};
        use opentelemetry_sdk::metrics::reader::MetricReader;
        use opentelemetry_sdk::metrics::{
            InstrumentKind, ManualReader, Pipeline, SdkMeterProvider, Temporality,
        };

        #[derive(Clone, Debug)]
        struct SharedReader(Arc<ManualReader>);

        impl MetricReader for SharedReader {
            fn register_pipeline(&self, pipeline: Weak<Pipeline>) {
                self.0.register_pipeline(pipeline);
            }

            fn collect(
                &self,
                metrics: &mut ResourceMetrics,
            ) -> opentelemetry_sdk::error::OTelSdkResult {
                self.0.collect(metrics)
            }

            fn force_flush(&self) -> opentelemetry_sdk::error::OTelSdkResult {
                self.0.force_flush()
            }

            fn shutdown_with_timeout(
                &self,
                timeout: std::time::Duration,
            ) -> opentelemetry_sdk::error::OTelSdkResult {
                self.0.shutdown_with_timeout(timeout)
            }

            fn temporality(&self, kind: InstrumentKind) -> Temporality {
                self.0.temporality(kind)
            }
        }

        #[derive(Default)]
        struct TestBudget {
            used: AtomicU64,
            limit: AtomicU64,
            refusals: AtomicU64,
        }

        impl VectorMemoryBudgetSource for TestBudget {
            fn vector_memory_budget_snapshot(&self) -> VectorMemoryBudgetSnapshot {
                VectorMemoryBudgetSnapshot {
                    used: self.used.load(Ordering::Relaxed),
                    limit: self.limit.load(Ordering::Relaxed),
                    refusals: self.refusals.load(Ordering::Relaxed),
                }
            }
        }

        let dead: Arc<dyn VectorMemoryBudgetSource> = Arc::new(TestBudget::default());
        let dead_id = register_vector_memory_budget(Arc::downgrade(&dead));
        drop(dead);

        let live: Arc<dyn VectorMemoryBudgetSource> = Arc::new(TestBudget {
            used: AtomicU64::new(17),
            limit: AtomicU64::new(101),
            refusals: AtomicU64::new(3),
        });
        let live_id = register_vector_memory_budget(Arc::downgrade(&live));
        let budgets = VECTOR_BUDGETS.get().unwrap().load();
        assert!(
            !budgets.iter().any(|(id, _)| *id == dead_id),
            "registration drops a budget that no longer exists"
        );
        assert!(budgets.iter().any(|(id, _)| *id == live_id));

        let reader = SharedReader(Arc::new(ManualReader::builder().build()));
        let provider = SdkMeterProvider::builder()
            .with_reader(reader.clone())
            .build();
        let installation = install(&provider);
        let mut metrics = ResourceMetrics::default();
        reader.collect(&mut metrics).unwrap();

        let metric = |name: &str| {
            metrics
                .scope_metrics()
                .flat_map(|scope| scope.metrics())
                .find(|metric| metric.name() == name)
                .unwrap_or_else(|| panic!("missing metric {name}"))
        };
        let gauge = |name: &str| {
            let metric = metric(name);
            let AggregatedMetrics::U64(MetricData::Gauge(gauge)) = metric.data() else {
                panic!("{name} is not a u64 gauge")
            };
            gauge
                .data_points()
                .find(|point| {
                    point.attributes().any(|attribute| {
                        attribute.key.as_str() == "budget"
                            && attribute.value.to_string() == live_id.to_string()
                    })
                })
                .unwrap_or_else(|| panic!("missing {name} for budget {live_id}"))
                .value()
        };
        let counter = |name: &str| {
            let metric = metric(name);
            let AggregatedMetrics::U64(MetricData::Sum(counter)) = metric.data() else {
                panic!("{name} is not a u64 counter")
            };
            counter
                .data_points()
                .find(|point| {
                    point.attributes().any(|attribute| {
                        attribute.key.as_str() == "budget"
                            && attribute.value.to_string() == live_id.to_string()
                    })
                })
                .unwrap_or_else(|| panic!("missing {name} for budget {live_id}"))
                .value()
        };

        assert_eq!(gauge("defradb.vector.memory.used"), 17);
        assert_eq!(gauge("defradb.vector.memory.limit"), 101);
        assert_eq!(counter("defradb.vector.memory.refusals"), 3);

        uninstall(installation);
        provider.shutdown().unwrap();
    }
}
