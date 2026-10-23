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
//! TxnTurn (trait)                <--   OwnedTxn (impl)
//! TransactionContext (trait)     <--   DbTransactionContext (impl)
//! ```
//!
//! Each explicit transaction is owned by one worker of the registry's pool
//! for its whole life ([`crate::txn::owner`]): its ID names the worker, every
//! request, commit, discard and abandon for it is a message to that worker,
//! and the worker runs them one at a time in the order they arrived. The
//! registry is the workers' own tables, nothing shared besides.

use defra_core::thread_bounds::MaybeSend;
use defra_core::RequestContext;
use futures::future::LocalBoxFuture;
use query::error::TransactionError;
use std::sync::Arc;
use std::time::Duration;
use storage::corekv::{IterOptions, Key, Store};

use crate::collection::Collection;
use crate::database::DB;
use crate::error::{Error, Result};
use crate::txn::fence::CommitFences;
use crate::txn::owner::{Owners, TxnId, Unavailable};

mod blocks;
mod cleanup;
mod collections;
mod docs;
mod lenses;
mod lifecycle;
pub(crate) mod owned;
mod schemas;

use owned::OwnedTxn;

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
/// Implements `query::TransactionRegistry`. The transactions live on the
/// workers of a [`workers::Pool`]: the one given at construction, else the
/// database's, else the process's default pool. In a browser the page's
/// one worker owns them all.
pub struct DbTransactionRegistry<S: Store + 'static> {
    db: Arc<DB<S>>,
    /// The workers' tables; an error when no pool could be had, which every
    /// request then reports.
    owners: std::result::Result<Owners<OwnedTxn<S>, Caller>, workers::Error>,
    broadcaster: Option<Arc<dyn crate::event::emission::TxnBroadcaster>>,
}

impl<S: Store + 'static> DbTransactionRegistry<S> {
    /// A registry whose transactions live on the database's pool.
    ///
    /// Use `with_broadcaster` when running with the P2P stack so that
    /// committed transactional writes are forwarded to peers.
    pub fn new(db: Arc<DB<S>>) -> Self {
        let pool = pool_of(&db);
        Self::on(db, pool, None)
    }

    /// Create a transaction registry that forwards committed writes to a
    /// `TxnBroadcaster`. Each contained transaction's success callbacks will
    /// call `broadcaster.broadcast_update` in addition to publishing to the
    /// local event bus.
    pub fn with_broadcaster(
        db: Arc<DB<S>>,
        broadcaster: Arc<dyn crate::event::emission::TxnBroadcaster>,
    ) -> Self {
        let pool = pool_of(&db);
        Self::on(db, pool, Some(broadcaster))
    }

    /// A registry whose transactions live on the workers of `pool`, each
    /// owned by the worker that began it.
    pub fn with_pool(
        db: Arc<DB<S>>,
        pool: workers::Pool,
        broadcaster: Option<Arc<dyn crate::event::emission::TxnBroadcaster>>,
    ) -> Self {
        Self::on(db, Ok(pool), broadcaster)
    }

    fn on(
        db: Arc<DB<S>>,
        pool: std::result::Result<workers::Pool, workers::Error>,
        broadcaster: Option<Arc<dyn crate::event::emission::TxnBroadcaster>>,
    ) -> Self {
        Self {
            owners: owners_on(pool, db.commit_fences()),
            db,
            broadcaster,
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

    /// Get the number of active transactions in the registry.
    pub fn active_transaction_count(&self) -> Result<usize> {
        Ok(self.owners.as_ref().map_or(0, Owners::len))
    }

    fn owners(&self) -> std::result::Result<&Owners<OwnedTxn<S>, Caller>, Unavailable> {
        self.owners
            .as_ref()
            .map_err(|error| Unavailable::Pool(error.clone()))
    }

    /// Runs `work` on transaction `txn_id`, on the worker that owns it, after
    /// every request for it that arrived earlier, for the request `context`
    /// serves.
    async fn in_txn<T, W>(&self, context: &RequestContext, txn_id: &str, work: W) -> Result<T>
    where
        T: MaybeSend + 'static,
        W: for<'t> FnOnce(&'t mut OwnedTxn<S>) -> LocalBoxFuture<'t, Result<T>>
            + MaybeSend
            + 'static,
    {
        let not_found = || Error::TransactionNotFound(txn_id.to_string());
        let id = TxnId::parse(txn_id).ok_or_else(not_found)?;
        let scope = context.clone();
        let ran = self
            .owners()
            .map_err(|error| Error::Other(error.to_string()))?
            .run(id, &caller(context), move |owned| {
                Box::pin(async move { scope.mirror(work(owned)).await })
            })
            .await;
        match ran {
            Ok(result) => result,
            Err(Unavailable::NotFound) => Err(not_found()),
            Err(error) => Err(Error::Other(error.to_string())),
        }
    }
}

/// Who began a transaction, and so may use it: the request's identity.
type Caller = Option<String>;

/// The identity acting in `context`, the request calling.
fn caller(context: &RequestContext) -> Caller {
    context.identity().map(str::to_owned)
}

/// A registry error, as the query layer reports it.
fn unavailable(error: Unavailable, handle: &query::txn::TransactionHandle) -> TransactionError {
    match error {
        Unavailable::NotFound => {
            TransactionError::not_found(format!("transaction '{handle}' not found"))
        }
        other => TransactionError::execution(other.to_string()),
    }
}

/// The pool a registry built without one runs on: the database's, else the
/// process's default pool (a test, a one-shot tool).
fn pool_of<S: Store>(db: &DB<S>) -> std::result::Result<workers::Pool, workers::Error> {
    db.pool_or_default()
}

#[cfg(not(target_family = "wasm"))]
fn owners_on<T, K>(
    pool: std::result::Result<workers::Pool, workers::Error>,
    fences: &Arc<CommitFences>,
) -> std::result::Result<Owners<T, K>, workers::Error>
where
    T: defra_core::thread_bounds::MaybeSend + 'static,
    K: Clone + PartialEq + Send + Sync + 'static,
{
    if let Err(error) = &pool {
        tracing::error!(%error, "no worker can own transactions; every transaction request will fail");
    }
    pool.map(|pool| Owners::fenced(pool, Arc::clone(fences)))
}

/// The page's one worker owns every transaction, whichever pool drives it.
#[cfg(target_family = "wasm")]
fn owners_on<T, K>(
    _pool: std::result::Result<workers::Pool, workers::Error>,
    fences: &Arc<CommitFences>,
) -> std::result::Result<Owners<T, K>, workers::Error>
where
    T: 'static,
    K: Clone + PartialEq + Send + Sync + 'static,
{
    Ok(Owners::here(Arc::clone(fences)))
}
