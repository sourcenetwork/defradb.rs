//! Explicit transactions begun and finalized for the caller `context` names.

use std::sync::Arc;

use defra_core::{RequestContext, TxnId};
use query::{TransactionError, TransactionHandle};

use super::NodeCore;
use crate::ops::transaction_error;
use crate::transaction::OwnedTransaction;
use crate::{refused_transaction, Error, Result};

/// A transaction [`NodeCore::begin`] began: abandoned when dropped unfinalized.
pub(crate) struct Begun {
    pub(crate) transaction: OwnedTransaction,
    pub(crate) id: TxnId,
    /// Who owns it, and so who commits or discards it.
    pub(crate) owner: RequestContext,
}

impl Begun {
    /// Stop owning the transaction, leaving it open, and return its ID.
    pub(crate) fn detach(self) -> TxnId {
        let _released = self.transaction.into_handle();
        self.id
    }
}

impl NodeCore<'_> {
    /// Begin an explicit transaction owned by the caller `context` names, or
    /// the node's principal when it names none. Begun as a request: a
    /// shutting-down node refuses it.
    pub(crate) async fn begin(&self, context: &RequestContext, read_only: bool) -> Result<Begun> {
        let owner = self.node.txn_owner(context);
        let transaction = self
            .node
            .begin_owned_transaction(owner.clone(), read_only)
            .await
            .map_err(transaction_error)?;
        let handle = transaction.handle().map_err(transaction_error)?;
        let id = handle.parse().map_err(|_| {
            Error::Transaction(TransactionError::Operation(format!(
                "transaction ID '{handle}' is not a number"
            )))
        })?;
        Ok(Begun {
            transaction,
            id,
            owner,
        })
    }

    /// Make the writes of transaction `id` durable and end it, as the caller
    /// `context` names. Admitted as a request, so a shutting-down node
    /// refuses it uncommitted.
    pub(crate) async fn commit(&self, context: &RequestContext, id: TxnId) -> Result<()> {
        let (runner, caller) = (Arc::clone(self.node.runner()), self.node.txn_owner(context));
        self.node
            .as_request(async move { runner.commit_txn_as(&caller, &handle(id)).await })
            .await
            .unwrap_or_else(|error| Err(refused_transaction(error, "committed")))
            .map_err(transaction_error)
    }

    /// Commit a transaction this scope began, as its owner. The commit task
    /// owns the transaction until it completes, so a cancelled caller does
    /// not abandon it; a refused or failed commit abandons it.
    pub(crate) async fn commit_begun(&self, begun: Begun) -> Result<()> {
        self.node
            .as_request(begun.transaction.commit(begun.owner))
            .await
            .unwrap_or_else(|error| Err(refused_transaction(error, "committed")))
            .map_err(transaction_error)
    }

    /// Drop the writes of transaction `id` and end it, as the caller
    /// `context` names.
    pub(crate) async fn discard(&self, context: &RequestContext, id: TxnId) -> Result<()> {
        self.node
            .runner()
            .discard_txn_as(&self.node.txn_owner(context), &handle(id))
            .await
            .map_err(transaction_error)
    }

    /// Discard a transaction this scope began, as its owner.
    pub(crate) async fn discard_begun(&self, begun: Begun) -> Result<()> {
        begun
            .transaction
            .discard(begun.owner)
            .await
            .map_err(transaction_error)
    }
}

fn handle(id: TxnId) -> TransactionHandle {
    TransactionHandle::new(id.to_string())
}
