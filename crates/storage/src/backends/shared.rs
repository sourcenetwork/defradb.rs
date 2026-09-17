//! Pieces every store needs that are not the store itself: transaction
//! lifecycle callbacks, and the diagnostics a node reports.
//!
//! What used to live here was a second concurrency-control implementation
//! layered over backends that had none: a conflict tracker, a read set, a
//! publication gate, and the version bookkeeping to drive them. regolith
//! validates its own transactions, so all of it is gone. What remains is
//! the callback bookkeeping DefraDB's transaction API promises, and
//! counters for what actually happened.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use kovan_queue::seg_queue::SegQueue;
use serde::{Deserialize, Serialize};

use crate::corekv::{AsyncTxnCallback, TxnCallback};

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

/// How many callbacks of each kind a transaction is carrying.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CallbackCounts {
    /// Synchronous success callbacks.
    pub success: usize,
    /// Asynchronous success callbacks.
    pub success_async: usize,
    /// Synchronous error callbacks.
    pub error: usize,
    /// Asynchronous error callbacks.
    pub error_async: usize,
    /// Synchronous discard callbacks.
    pub discard: usize,
    /// Asynchronous discard callbacks.
    pub discard_async: usize,
}

impl CallbackCounts {
    /// Callbacks registered across every kind.
    pub fn total(&self) -> usize {
        self.success
            + self.success_async
            + self.error
            + self.error_async
            + self.discard
            + self.discard_async
    }
}

/// Callbacks a transaction runs when it resolves.
///
/// Registration takes `&self` because a transaction is shareable. Each list
/// is a FIFO queue so callbacks run in registration order without a lock.
#[derive(Default)]
pub(crate) struct CallbackManager {
    success: SegQueue<TxnCallback>,
    success_async: SegQueue<AsyncTxnCallback>,
    error: SegQueue<TxnCallback>,
    error_async: SegQueue<AsyncTxnCallback>,
    discard: SegQueue<TxnCallback>,
    discard_async: SegQueue<AsyncTxnCallback>,
}

impl CallbackManager {
    pub(crate) fn on_success(&self, callback: TxnCallback) {
        self.success.push(callback);
    }

    pub(crate) fn on_success_async(&self, callback: AsyncTxnCallback) {
        self.success_async.push(callback);
    }

    pub(crate) fn on_error(&self, callback: TxnCallback) {
        self.error.push(callback);
    }

    pub(crate) fn on_error_async(&self, callback: AsyncTxnCallback) {
        self.error_async.push(callback);
    }

    pub(crate) fn on_discard(&self, callback: TxnCallback) {
        self.discard.push(callback);
    }

    pub(crate) fn on_discard_async(&self, callback: AsyncTxnCallback) {
        self.discard_async.push(callback);
    }

    pub(crate) fn counts(&self) -> CallbackCounts {
        CallbackCounts {
            success: self.success.len(),
            success_async: self.success_async.len(),
            error: self.error.len(),
            error_async: self.error_async.len(),
            discard: self.discard.len(),
            discard_async: self.discard_async.len(),
        }
    }

    pub(crate) fn total(&self) -> usize {
        self.counts().total()
    }

    /// Run the success callbacks, synchronous ones first.
    pub(crate) async fn run_success(&self) {
        Self::run(&self.success);
        while let Some(callback) = self.success_async.pop() {
            callback().await;
        }
    }

    /// Run the error callbacks.
    pub(crate) async fn run_error(&self) {
        Self::run(&self.error);
        while let Some(callback) = self.error_async.pop() {
            callback().await;
        }
    }

    /// Run the discard callbacks.
    ///
    /// `discard` is synchronous in DefraDB's API, so an async callback
    /// cannot be awaited here. On a runtime it is spawned, which is the
    /// long-standing fire-and-forget contract. Without one there is
    /// nowhere to run it, and that is said out loud rather than dropped
    /// quietly.
    pub(crate) fn run_discard(&self) {
        Self::run(&self.discard);
        if self.discard_async.is_empty() {
            return;
        }
        let mut pending = Vec::with_capacity(self.discard_async.len());
        while let Some(callback) = self.discard_async.pop() {
            pending.push(callback);
        }
        #[cfg(not(target_arch = "wasm32"))]
        match tokio::runtime::Handle::try_current() {
            Ok(handle) => {
                handle.spawn(async move {
                    for callback in pending {
                        callback().await;
                    }
                });
            }
            Err(_) => tracing::warn!(
                count = pending.len(),
                "async discard callbacks dropped: discard() ran outside a tokio runtime"
            ),
        }
        #[cfg(target_arch = "wasm32")]
        tracing::warn!(
            count = pending.len(),
            "async discard callbacks dropped: no runtime to spawn them on"
        );
    }

    fn run(callbacks: &SegQueue<TxnCallback>) {
        while let Some(callback) = callbacks.pop() {
            callback();
        }
    }
}

/// What a store reports about its transactions.
///
/// Deliberately small. regolith reports that a commit conflicted, not
/// which dependency edge caused it, so there is no per-rule breakdown
/// here: a field that could only ever be zero would read as a measurement
/// rather than an absence.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct TransactionStatsSnapshot {
    /// Backend that owns these numbers.
    pub backend: &'static str,
    /// Transactions that committed.
    pub commits: u64,
    /// Transactions the engine refused at commit because their read or
    /// write set had moved underneath them.
    pub conflicts: u64,
}

#[derive(Default)]
struct TransactionMetrics {
    commits: AtomicU64,
    conflicts: AtomicU64,
}

/// Cloneable handle for reading a live store's transaction diagnostics.
#[derive(Clone)]
pub struct TransactionStatsHandle {
    backend: &'static str,
    metrics: Arc<TransactionMetrics>,
}

impl TransactionStatsHandle {
    pub(crate) fn for_backend(backend: &'static str) -> Self {
        Self {
            backend,
            metrics: Arc::new(TransactionMetrics::default()),
        }
    }

    pub(crate) fn record_commit(&self) {
        self.metrics.commits.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn record_conflict(&self) {
        self.metrics.conflicts.fetch_add(1, Ordering::Relaxed);
    }

    /// Read the counters as of now.
    pub fn snapshot(&self) -> TransactionStatsSnapshot {
        TransactionStatsSnapshot {
            backend: self.backend,
            commits: self.metrics.commits.load(Ordering::Relaxed),
            conflicts: self.metrics.conflicts.load(Ordering::Relaxed),
        }
    }
}
