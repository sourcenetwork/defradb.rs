//! Transaction registry for query execution.
//!
//! This module provides the `DbTransactionRegistry` which implements the query crate's
//! `TransactionRegistry` trait, enabling transaction-aware query execution.
//!
//! # Architecture
//!
//! ```text
//! query crate                          db crate
//! ───────────                          ────────
//! TransactionRegistry (trait)    <--   DbTransactionRegistry (impl)
//! TransactionContext (trait)     <--   DbTransactionContext (impl)
//! DocFetcher (trait)             <--   DbDocFetcher (impl)
//! ```

use async_trait::async_trait;
use document::Document;
use kovan_map::HopscotchMap;
use kovan_queue::seg_queue::SegQueue;
use lens::{LensConfig, LensModule, TransformId};
use query::error::TransactionError;
use query::txn::{
    DeferredAcpMutations, GetTransactionResult, TransactionContext, TransactionHandle,
    TransactionRegistry,
};
use rapidhash::fast::RandomState;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Weak};
use std::time::Duration;
use storage::corekv::{IterOptions, Key, Store};
use tracing::{error, warn};
use web_time::Instant;

use crate::collection::Collection;
use crate::database::DB;
use crate::error::{Error, Result};
use crate::read::lensed::fetcher::LensedDocFetcher;
use crate::txn::context::DbTransactionContext;
use crate::txn::lenses::TxnLensStore;

mod blocks;
mod cleanup;
mod collections;
mod docs;
mod lenses;
mod lifecycle;
mod schemas;

/// Default max idle age for explicit HTTP transactions.
pub const DEFAULT_TRANSACTION_IDLE_TIMEOUT: Duration = Duration::from_secs(600);

/// Default interval between explicit HTTP transaction cleanup sweeps.
pub const DEFAULT_TRANSACTION_CLEANUP_INTERVAL: Duration = Duration::from_secs(60);

/// Result of a stale transaction cleanup operation.
///
/// Provides visibility into both successful cleanups and failures,
/// allowing callers to monitor for resource leaks.
#[derive(Debug, Clone, Default)]
pub struct CleanupResult {
    /// Number of transactions successfully cleaned up.
    pub cleaned: usize,
    /// Transactions that failed to clean up: (transaction_id, error_message).
    pub failed: Vec<(String, String)>,
}

impl CleanupResult {
    /// Returns true if all cleanup operations succeeded.
    pub fn is_complete(&self) -> bool {
        self.failed.is_empty()
    }

    /// Total number of transactions that were attempted to be cleaned.
    pub fn attempted(&self) -> usize {
        self.cleaned + self.failed.len()
    }
}

/// Transaction registry that manages database transactions for query execution.
///
/// Implements `query::TransactionRegistry` to provide transaction lifecycle
/// management to the query executor.
///
/// # Thread Safety
///
/// The transaction map is lock-free so lookups stay synchronous (required by
/// the trait); the underlying transaction keeps its async mutex to support
/// async document fetching operations.
pub struct DbTransactionRegistry<S: Store + 'static> {
    db: Arc<DB<S>>,
    transactions: HopscotchMap<String, Arc<TransactionSlot<S>>, RandomState>,
    id_counter: AtomicU64,
    broadcaster: Option<Arc<dyn crate::event::emission::TxnBroadcaster>>,
}

/// A registered transaction: the single owning reference plus a borrowing handle.
///
/// The map retires a removed entry instead of dropping it, so the owning
/// reference lives in a queue whose `pop` hands it back by value at the
/// removal. The context, and with it the `Arc<DB<S>>` holding the store's
/// on-disk lock, is then released where the transaction leaves the registry
/// rather than whenever reclamation catches up.
struct TransactionSlot<S: Store + 'static> {
    owner: SegQueue<Arc<DbTransactionContext<S>>>,
    reader: Weak<DbTransactionContext<S>>,
}

impl<S: Store + 'static> TransactionSlot<S> {
    fn new(ctx: Arc<DbTransactionContext<S>>) -> Self {
        let reader = Arc::downgrade(&ctx);
        let owner = SegQueue::new();
        owner.push(ctx);
        Self { owner, reader }
    }

    fn ctx(&self) -> Option<Arc<DbTransactionContext<S>>> {
        self.reader.upgrade()
    }
}

impl<S: Store + 'static> DbTransactionRegistry<S> {
    /// Create a new transaction registry without a P2P broadcaster.
    ///
    /// Use `with_broadcaster` when running with the P2P stack so that
    /// committed transactional writes are forwarded to peers.
    pub fn new(db: Arc<DB<S>>) -> Self {
        Self {
            db,
            transactions: HopscotchMap::with_hasher(RandomState::default()),
            id_counter: AtomicU64::new(0),
            broadcaster: None,
        }
    }

    /// Create a transaction registry that forwards committed writes to a
    /// `TxnBroadcaster`. Each contained transaction's success callbacks will
    /// call `broadcaster.broadcast_update` in addition to publishing to the
    /// local event bus.
    pub fn with_broadcaster(
        db: Arc<DB<S>>,
        broadcaster: Arc<dyn crate::event::emission::TxnBroadcaster>,
    ) -> Self {
        Self {
            db,
            transactions: HopscotchMap::with_hasher(RandomState::default()),
            id_counter: AtomicU64::new(0),
            broadcaster: Some(broadcaster),
        }
    }

    /// Get the database instance.
    pub fn db(&self) -> &Arc<DB<S>> {
        &self.db
    }

    /// Get all collection names from the DB.
    ///
    /// Uses the process-wide cache. For transaction-scoped access,
    /// use the transaction's collection cache directly.
    pub fn collection_names(&self) -> Result<Vec<String>> {
        self.db.list_collections()
    }

    /// Get a collection by name from the DB.
    ///
    /// Uses the process-wide cache. For transaction-scoped access,
    /// use the transaction's collection cache directly.
    pub fn collection(&self, name: &str) -> Result<Option<Collection>> {
        self.db.get_collection(name)
    }

    /// Get an existing transaction by ID (for internal use).
    ///
    /// Returns `Ok(None)` if the transaction doesn't exist, belongs to another
    /// caller, or a cleanup sweep has already claimed it.
    pub fn get_ctx(&self, txn_id: &str) -> Result<Option<Arc<DbTransactionContext<S>>>> {
        Ok(self
            .transactions
            .get(txn_id)
            .and_then(|slot| slot.ctx())
            .filter(|ctx| ctx.is_owned_by_caller() && ctx.touch()))
    }

    /// Get the number of active transactions in the registry.
    pub fn active_transaction_count(&self) -> Result<usize> {
        Ok(self.transactions.len())
    }

    /// Unregister a transaction, taking ownership of releasing it.
    fn take_registered(&self, handle: &TransactionHandle) -> Option<RemovedTransaction<S>> {
        self.transactions
            .remove(handle.as_str())?
            .owner
            .pop()
            .map(RemovedTransaction)
    }
}

/// A transaction context the registry no longer holds.
///
/// A caller that never reaches the commit or rollback (an abandoned handle, a
/// cancelled finalization) would otherwise leave the transaction open and the
/// store locked. Dropping this releases it.
struct RemovedTransaction<S: Store + 'static>(Arc<DbTransactionContext<S>>);

impl<S: Store + 'static> std::ops::Deref for RemovedTransaction<S> {
    type Target = DbTransactionContext<S>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<S: Store + 'static> Drop for RemovedTransaction<S> {
    fn drop(&mut self) {
        if let Some(txn) = self.0.try_take_txn() {
            if let Err(error) = txn.force_discard() {
                error!(error = %error, "Failed to discard an unfinalized transaction");
            }
        }
    }
}
