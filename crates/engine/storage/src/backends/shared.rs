//! Pieces every store needs that are not the store itself: the durability
//! mode, and the diagnostics a node reports.
//!
//! What used to live here was a second concurrency-control implementation
//! layered over backends that had none: a conflict tracker, a read set, a
//! publication gate, and the version bookkeeping to drive them. regolith
//! validates its own transactions, so all of it is gone, and a
//! transaction's callbacks live in one registry
//! ([`TxnCallbacks`](crate::corekv::TxnCallbacks)). What remains is counters
//! for what actually happened.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

/// When a write is made durable.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum DurabilityMode {
    /// Fsync before the commit returns, so a committed write survives a
    /// power cut.
    #[default]
    Immediate,
    /// Hand the write to the kernel without an fsync. Survives a process
    /// crash, not a power cut.
    Eventual,
}

/// Engine-owned commit/policy counters and listener-owned conflict reasons.
/// Snapshots are observational: concurrent commits may advance between reads.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct TransactionStatsSnapshot {
    pub backend: &'static str,
    /// Native transaction commits; read-only adapter snapshots do not commit.
    pub commits: u64,
    pub conflicts: u64,
    /// Regolith's commit.* and policy.* tickers, using their native names.
    pub engine: std::collections::BTreeMap<&'static str, u64>,
    /// Conflict counts by losing access and winning write, without key data.
    pub conflict_reasons: std::collections::BTreeMap<String, u64>,
    /// Calls on a store placed on a worker pool that were made off its
    /// workers: each borrowed a worker's queue for its I/O instead of running
    /// where its request was routed. Zero when every caller is routed.
    #[serde(skip_serializing_if = "is_zero")]
    pub off_pool_calls: u64,
}

fn is_zero(count: &u64) -> bool {
    *count == 0
}

const ACCESS_LABELS: [&str; 10] = [
    "read",
    "read_parts",
    "read_presence",
    "read_for_update",
    "scanned_then_wrote",
    "scanned_range",
    "put",
    "delete",
    "merge",
    "other",
];
const WRITE_LABELS: [&str; 5] = ["put", "delete", "range_delete", "merge", "other"];

struct TransactionMetrics {
    engine: Arc<regolith::Statistics>,
    reasons: [[AtomicU64; 5]; 10],
    off_pool: AtomicU64,
    /// The first off-pool call was reported; later ones are only counted.
    off_pool_reported: std::sync::atomic::AtomicBool,
    /// Held, never read: the engine's tickers stay in the OpenTelemetry
    /// export while this store's diagnostics are reachable.
    _export: telemetry::EngineTickerExport,
}

/// Cloneable handle for reading a live store's engine diagnostics.
#[derive(Clone)]
pub struct TransactionStatsHandle {
    metrics: Arc<TransactionMetrics>,
}

impl TransactionStatsHandle {
    pub(crate) fn from_engine(engine: Arc<regolith::Statistics>) -> Self {
        Self {
            metrics: Arc::new(TransactionMetrics {
                _export: telemetry::export_engine_tickers(&engine),
                engine,
                reasons: std::array::from_fn(|_| std::array::from_fn(|_| AtomicU64::new(0))),
                off_pool: AtomicU64::new(0),
                off_pool_reported: std::sync::atomic::AtomicBool::new(false),
            }),
        }
    }

    pub(crate) fn record_conflict(&self, conflict: &regolith::Conflict) {
        use regolith::{Access, WriteKind};
        let mine = match conflict.mine() {
            Access::Read => 0,
            Access::ReadParts => 1,
            Access::ReadPresence => 2,
            Access::ReadForUpdate => 3,
            Access::ScannedThenWrote => 4,
            Access::ScannedRange => 5,
            Access::Put => 6,
            Access::Delete => 7,
            Access::Merge => 8,
            _ => 9,
        };
        let theirs = match conflict.theirs() {
            WriteKind::Put => 0,
            WriteKind::Delete => 1,
            WriteKind::RangeDelete => 2,
            WriteKind::Merge => 3,
            _ => 4,
        };
        self.metrics.reasons[mine][theirs].fetch_add(1, Ordering::Relaxed);
        telemetry::record_storage_conflict("regolith", ACCESS_LABELS[mine], WRITE_LABELS[theirs]);
    }

    /// A call on a store placed on a worker pool was made off its workers.
    /// Reported once, counted every time.
    pub(crate) fn record_off_pool(&self, call: &'static str) {
        self.metrics.off_pool.fetch_add(1, Ordering::Relaxed);
        if !self.metrics.off_pool_reported.swap(true, Ordering::Relaxed) {
            tracing::warn!(
                call,
                "a storage call ran off the worker pool its store is placed on; route it through the pool"
            );
        }
    }

    pub fn snapshot(&self) -> TransactionStatsSnapshot {
        let engine = &self.metrics.engine;
        let mut conflict_reasons = std::collections::BTreeMap::new();
        for (mine, access) in ACCESS_LABELS.iter().enumerate() {
            for (theirs, write) in WRITE_LABELS.iter().enumerate() {
                let count = self.metrics.reasons[mine][theirs].load(Ordering::Relaxed);
                if count != 0 {
                    conflict_reasons.insert(format!("{access}/{write}"), count);
                }
            }
        }
        TransactionStatsSnapshot {
            backend: "regolith",
            commits: engine.get_ticker(regolith::Ticker::CommitCount),
            conflicts: engine.get_ticker(regolith::Ticker::CommitConflicts),
            engine: telemetry::exported_engine_tickers()
                .map(|ticker| (ticker.name(), engine.get_ticker(ticker)))
                .collect(),
            conflict_reasons,
            off_pool_calls: self.metrics.off_pool.load(Ordering::Relaxed),
        }
    }
}
