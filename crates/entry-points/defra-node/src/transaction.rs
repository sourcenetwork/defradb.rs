use std::sync::Arc;

use defra_core::RequestContext;
use query::TransactionError;

use crate::{QueryExecutor, TransactionHandle};

/// Owns a transaction while its begin result or commit task is unobserved.
pub(crate) struct OwnedTransaction {
    executor: Arc<dyn QueryExecutor>,
    handle: Option<TransactionHandle>,
}

impl OwnedTransaction {
    pub(crate) fn new(executor: Arc<dyn QueryExecutor>, handle: TransactionHandle) -> Self {
        Self {
            executor,
            handle: Some(handle),
        }
    }

    pub(crate) fn handle(&self) -> Result<&TransactionHandle, TransactionError> {
        self.handle
            .as_ref()
            .ok_or_else(|| TransactionError::already_finalized("transaction already finalized"))
    }

    pub(crate) fn into_handle(mut self) -> Result<TransactionHandle, TransactionError> {
        self.handle
            .take()
            .ok_or_else(|| TransactionError::already_finalized("transaction already finalized"))
    }

    pub(crate) async fn commit(mut self, context: RequestContext) -> Result<(), TransactionError> {
        self.executor
            .commit_txn_as(&context, self.handle()?)
            .await?;
        self.handle = None;
        Ok(())
    }

    /// Roll back the writes as `context`; a failed discard leaves the
    /// transaction to be abandoned on drop.
    pub(crate) async fn discard(mut self, context: RequestContext) -> Result<(), TransactionError> {
        self.executor
            .discard_txn_as(&context, self.handle()?)
            .await?;
        self.handle = None;
        Ok(())
    }
}

impl Drop for OwnedTransaction {
    fn drop(&mut self) {
        if let Some(handle) = &self.handle {
            self.executor.abandon_txn(handle);
        }
    }
}

#[cfg(test)]
mod cancellation_tests;
