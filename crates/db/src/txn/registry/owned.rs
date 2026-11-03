//! An explicit transaction as its owner holds it: by value, between requests,
//! and lent to the handles of the one request running on it.
//!
//! A request borrows the transaction through [`TxnTurn::context`]: the
//! transaction moves behind one shared reference for the request's handles.
//! Whatever needs it held alone (a statement savepoint, a registry request
//! that changes definitions, the commit) takes it back first, which works
//! once the request's handles are dropped and fails loud, never waits, while
//! one is still open. Dropping an owned transaction discards it, whatever
//! ended it: an abandon, the idle sweep, a dropped registry, a stopped pool.

use std::sync::Arc;

use query::error::TransactionError;
use query::txn::{TransactionContext, TxnTurn};
use storage::corekv::Store;
use tracing::{error, warn};

use crate::database::DB;
use crate::error::{Error, Result};
use crate::event::emission::TxnBroadcaster;
use crate::read::lensed::fetcher::FetcherCaches;
use crate::txn::context::DbTransactionContext;
use crate::txn::lenses::TxnLensStore;
use crate::txn::owner::TxnId;
use crate::txn::{DbTxn, LentTxn};

// One per transaction, held by value: boxing the held transaction would
// allocate once per transaction for nothing.
#[allow(clippy::large_enum_variant)]
enum Lending<S>
where
    S: Store,
{
    Held(DbTxn<S>),
    Lent(LentTxn<S>),
    Gone,
}

/// One explicit transaction, owned by the worker that began it.
pub(crate) struct OwnedTxn<S>
where
    S: Store + 'static,
{
    id: TxnId,
    readonly: bool,
    txn: Lending<S>,
    db: Arc<DB<S>>,
    lens_store: Arc<TxnLensStore>,
    caches: FetcherCaches,
    broadcaster: Option<Arc<dyn TxnBroadcaster>>,
    /// A request opened a statement it has not closed yet.
    statement_open: bool,
}

impl<S> OwnedTxn<S>
where
    S: Store + 'static,
{
    pub(crate) fn new(
        id: TxnId,
        txn: DbTxn<S>,
        db: Arc<DB<S>>,
        broadcaster: Option<Arc<dyn TxnBroadcaster>>,
    ) -> Result<Self> {
        let readonly = txn.is_readonly()?;
        Ok(Self {
            id,
            readonly,
            lens_store: Arc::new(TxnLensStore::new(db.lens_store().clone())),
            txn: Lending::Held(txn),
            db,
            caches: FetcherCaches::default(),
            broadcaster,
            statement_open: false,
        })
    }

    /// The transaction for a request's handles to share.
    pub(crate) fn lend(&mut self) -> Result<LentTxn<S>> {
        let lent = match std::mem::replace(&mut self.txn, Lending::Gone) {
            Lending::Held(txn) => Arc::new(txn),
            Lending::Lent(txn) => txn,
            Lending::Gone => return Err(Error::TxnNotActive),
        };
        self.txn = Lending::Lent(Arc::clone(&lent));
        Ok(lent)
    }

    /// The transaction, held alone: every handle lent to a request is gone.
    pub(crate) fn alone(&mut self) -> Result<&mut DbTxn<S>> {
        if let Lending::Lent(_) = &self.txn {
            let Lending::Lent(lent) = std::mem::replace(&mut self.txn, Lending::Gone) else {
                return Err(Error::TxnNotActive);
            };
            match Arc::try_unwrap(lent) {
                Ok(txn) => self.txn = Lending::Held(txn),
                Err(lent) => {
                    self.txn = Lending::Lent(lent);
                    return Err(Error::TxnInUse);
                }
            }
        }
        match &mut self.txn {
            Lending::Held(txn) => Ok(txn),
            _ => Err(Error::TxnNotActive),
        }
    }

    /// Takes the transaction out, held alone, to end it.
    fn take(&mut self) -> Result<DbTxn<S>> {
        self.alone()?;
        match std::mem::replace(&mut self.txn, Lending::Gone) {
            Lending::Held(txn) => Ok(txn),
            _ => Err(Error::TxnNotActive),
        }
    }

    /// This transaction's lens registrations, visible to it alone until it
    /// commits.
    pub(crate) fn lens_store(&self) -> Arc<dyn lens::TransformStore> {
        self.lens_store.clone()
    }

    /// What its requests' fetchers derived from its schema.
    pub(crate) fn caches(&self) -> FetcherCaches {
        self.caches.clone()
    }

    /// After a request: a statement it opened and never closed (it failed,
    /// it was cancelled, its deadline passed) is undone.
    pub(crate) fn after_request(&mut self) {
        if !std::mem::take(&mut self.statement_open) {
            return;
        }
        let undone = self
            .alone()
            .and_then(|txn| txn.rollback_statement_savepoint());
        if let Err(error) = undone {
            error!(
                txn_id = %self.id,
                %error,
                "failed to undo the statement a request left open"
            );
        }
    }

    /// Commits the transaction, after every request queued before this.
    pub(crate) async fn commit(mut self) -> Result<()> {
        self.take()?.force_commit().await
    }

    /// Rolls the transaction back, after every request queued before this.
    pub(crate) fn discard(mut self) -> Result<()> {
        self.take()?.force_discard()
    }

    fn turn_error(error: Error) -> TransactionError {
        crate::error::transaction_error(error)
    }
}

impl<S> TxnTurn for OwnedTxn<S>
where
    S: Store + 'static,
{
    fn is_readonly(&self) -> bool {
        self.readonly
    }

    fn context(&mut self) -> std::result::Result<Arc<dyn TransactionContext>, TransactionError> {
        let txn = self.lend().map_err(Self::turn_error)?;
        Ok(Arc::new(DbTransactionContext::new(
            self.db.clone(),
            self.id.to_string(),
            self.readonly,
            &txn,
            self.lens_store(),
            self.caches(),
            self.broadcaster.clone(),
        )))
    }

    fn begin_statement(&mut self) -> std::result::Result<(), TransactionError> {
        self.alone()
            .and_then(|txn| txn.set_statement_savepoint())
            .map_err(Self::turn_error)?;
        self.statement_open = true;
        Ok(())
    }

    fn end_statement(&mut self, keep: bool) -> std::result::Result<(), TransactionError> {
        self.statement_open = false;
        self.alone()
            .and_then(|txn| {
                if keep {
                    txn.release_statement_savepoint()
                } else {
                    txn.rollback_statement_savepoint()
                }
            })
            .map_err(Self::turn_error)
    }
}

impl<S> Drop for OwnedTxn<S>
where
    S: Store + 'static,
{
    /// A transaction that ends without a commit or a discard is discarded,
    /// always.
    fn drop(&mut self) {
        let txn = match std::mem::replace(&mut self.txn, Lending::Gone) {
            Lending::Held(txn) => txn,
            Lending::Lent(lent) => match Arc::try_unwrap(lent) {
                Ok(txn) => txn,
                Err(_) => {
                    warn!(
                        txn_id = %self.id,
                        "a handle outlived its request; the transaction ends when it drops"
                    );
                    return;
                }
            },
            Lending::Gone => return,
        };
        if let Err(error) = txn.force_discard() {
            error!(txn_id = %self.id, %error, "failed to discard an unfinished transaction");
        }
    }
}
