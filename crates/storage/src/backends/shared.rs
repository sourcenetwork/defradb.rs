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
use std::sync::{Arc, OnceLock};

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

/// The six queues, allocated together the first time one is used.
#[derive(Default)]
struct CallbackLists {
    success: SegQueue<TxnCallback>,
    success_async: SegQueue<AsyncTxnCallback>,
    error: SegQueue<TxnCallback>,
    error_async: SegQueue<AsyncTxnCallback>,
    discard: SegQueue<TxnCallback>,
    discard_async: SegQueue<AsyncTxnCallback>,
}

/// Callbacks a transaction runs when it resolves.
///
/// Registration takes `&self` because a transaction is shareable. Each list
/// is a FIFO queue so callbacks run in registration order without a lock.
///
/// The queues are built on the first registration rather than with the
/// transaction. `SegQueue::new` allocates a 32-slot segment up front and
/// frees it through an epoch-pinned walk, and a transaction carries six of
/// them, so building them eagerly cost a read-only transaction (which
/// registers nothing) six allocations, six frees and seven epoch pins on a
/// path whose whole budget is a few hundred nanoseconds. Behind the box this
/// is two words in the transaction, and an unused one is an atomic load.
#[derive(Default)]
pub(crate) struct CallbackManager {
    lists: OnceLock<Box<CallbackLists>>,
}

impl CallbackManager {
    fn lists(&self) -> &CallbackLists {
        self.lists.get_or_init(Box::default)
    }

    fn registered(&self) -> Option<&CallbackLists> {
        self.lists.get().map(Box::as_ref)
    }

    pub(crate) fn on_success(&self, callback: TxnCallback) {
        self.lists().success.push(callback);
    }

    pub(crate) fn on_success_async(&self, callback: AsyncTxnCallback) {
        self.lists().success_async.push(callback);
    }

    pub(crate) fn on_error(&self, callback: TxnCallback) {
        self.lists().error.push(callback);
    }

    pub(crate) fn on_error_async(&self, callback: AsyncTxnCallback) {
        self.lists().error_async.push(callback);
    }

    pub(crate) fn on_discard(&self, callback: TxnCallback) {
        self.lists().discard.push(callback);
    }

    pub(crate) fn on_discard_async(&self, callback: AsyncTxnCallback) {
        self.lists().discard_async.push(callback);
    }

    pub(crate) fn counts(&self) -> CallbackCounts {
        let Some(lists) = self.registered() else {
            return CallbackCounts::default();
        };
        CallbackCounts {
            success: lists.success.len(),
            success_async: lists.success_async.len(),
            error: lists.error.len(),
            error_async: lists.error_async.len(),
            discard: lists.discard.len(),
            discard_async: lists.discard_async.len(),
        }
    }

    pub(crate) fn total(&self) -> usize {
        self.counts().total()
    }

    /// Run the success callbacks, synchronous ones first.
    pub(crate) async fn run_success(&self) {
        let Some(lists) = self.registered() else {
            return;
        };
        Self::run(&lists.success);
        while let Some(callback) = lists.success_async.pop() {
            callback().await;
        }
    }

    /// Run the error callbacks.
    pub(crate) async fn run_error(&self) {
        let Some(lists) = self.registered() else {
            return;
        };
        Self::run(&lists.error);
        while let Some(callback) = lists.error_async.pop() {
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
        let Some(lists) = self.registered() else {
            return;
        };
        Self::run(&lists.discard);
        if lists.discard_async.is_empty() {
            return;
        }
        let mut pending = Vec::with_capacity(lists.discard_async.len());
        while let Some(callback) = lists.discard_async.pop() {
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
