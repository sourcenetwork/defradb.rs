//! One request's turn on a transaction.
//!
//! A registry runs every request for an explicit transaction where the
//! transaction lives, one at a time, in the order the requests arrived
//! ([`TransactionRegistry::run`](super::TransactionRegistry::run)). The
//! request gets a [`TxnTurn`] for as long as it runs: it lends the
//! transaction to the request's handles ([`TxnTurn::context`]) and opens and
//! closes the request's statement, whose writes are undone if it fails or
//! never finishes. Opening or closing a statement needs the transaction held
//! alone, so every handle the request was lent must be dropped first.

use std::sync::Arc;

use defra_core::thread_bounds::{MaybeBoxFuture, MaybeSend};

use crate::error::TransactionError;

use super::context::TransactionContext;

/// One request's exclusive turn on a transaction.
pub trait TxnTurn: MaybeSend {
    /// Whether the transaction is read-only.
    fn is_readonly(&self) -> bool;

    /// The transaction, lent to this request's handles.
    fn context(&mut self) -> Result<Arc<dyn TransactionContext>, TransactionError>;

    /// Starts the request's statement: what it writes from here on is undone
    /// unless [`end_statement`](Self::end_statement) keeps it. A request that
    /// ends without ending its statement has it undone.
    fn begin_statement(&mut self) -> Result<(), TransactionError>;

    /// Ends the statement, keeping its writes if `keep`, undoing them
    /// otherwise.
    fn end_statement(&mut self, keep: bool) -> Result<(), TransactionError>;

    /// Pins `version`, a collection version the request selected (possibly
    /// inactive), for this turn, so every lookup of its name answers it; or
    /// clears the pin with `None`. A transaction that cannot pin refuses,
    /// rather than run the request against the active version.
    fn pin_collection(
        &mut self,
        version: Option<schema::CollectionVersion>,
    ) -> Result<(), TransactionError> {
        match version {
            None => Ok(()),
            Some(_) => Err(TransactionError::not_supported(
                "working through a selected collection version is not supported here",
            )),
        }
    }
}

/// The work of one request on a transaction, run where the transaction lives.
#[cfg(not(target_arch = "wasm32"))]
pub type TxnJob<T> =
    Box<dyn for<'t> FnOnce(&'t mut dyn TxnTurn) -> MaybeBoxFuture<'t, T> + Send + 'static>;
/// The work of one request on a transaction, run where the transaction lives.
#[cfg(target_arch = "wasm32")]
pub type TxnJob<T> =
    Box<dyn for<'t> FnOnce(&'t mut dyn TxnTurn) -> MaybeBoxFuture<'t, T> + 'static>;

/// Boxes `job` as a [`TxnJob`], its signature fixed for every borrow of the
/// turn.
#[cfg(not(target_arch = "wasm32"))]
pub fn txn_job<T, F>(job: F) -> TxnJob<T>
where
    F: for<'t> FnOnce(&'t mut dyn TxnTurn) -> MaybeBoxFuture<'t, T> + Send + 'static,
{
    Box::new(job)
}

/// Boxes `job` as a [`TxnJob`], its signature fixed for every borrow of the
/// turn.
#[cfg(target_arch = "wasm32")]
pub fn txn_job<T, F>(job: F) -> TxnJob<T>
where
    F: for<'t> FnOnce(&'t mut dyn TxnTurn) -> MaybeBoxFuture<'t, T> + 'static,
{
    Box::new(job)
}

/// A turn over a context that has no statements of its own to undo: every
/// request gets the same context, and a statement keeps or undoes nothing.
/// For registries that hold no storage transaction (tests, mocks).
pub struct SharedTurn(pub Arc<dyn TransactionContext>);

impl TxnTurn for SharedTurn {
    fn is_readonly(&self) -> bool {
        self.0.is_readonly()
    }

    fn context(&mut self) -> Result<Arc<dyn TransactionContext>, TransactionError> {
        Ok(Arc::clone(&self.0))
    }

    fn begin_statement(&mut self) -> Result<(), TransactionError> {
        Ok(())
    }

    fn end_statement(&mut self, _keep: bool) -> Result<(), TransactionError> {
        Ok(())
    }
}
