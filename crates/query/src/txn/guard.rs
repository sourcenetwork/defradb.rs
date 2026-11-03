//! Owning transaction guard with synchronous cancellation cleanup.

use defra_core::RequestContext;

use crate::error::TransactionError;

use super::TransactionHandle;

/// A guard that ensures a transaction is properly finalized.
///
/// This type provides compile-time safety by consuming itself on `commit()` or
/// `rollback()`, preventing use-after-finalization bugs.
///
/// Dropping the guard removes the registry entry without committing, including
/// when its task is aborted or the async runtime has stopped. Cloned handles
/// are non-owning IDs and do not keep the entry alive. In-flight context users
/// may delay storage release until they also finish or are dropped.
///
/// Cancellation before finalization discards uncommitted writes. Once a commit
/// becomes durable it cannot be undone, even if cancellation interrupts its
/// post-commit callbacks. A cancelled commit therefore has an unknown outcome;
/// do not blindly retry non-idempotent writes. A competing explicit finalizer
/// that already removed the entry owns that outcome.
///
/// # Example
///
/// ```ignore
/// use query::{QueryExecutor, QueryRequest, TransactionGuard};
/// use query::error::TransactionError;
///
/// async fn batch_operations<E: QueryExecutor>(
///     executor: &E,
///     queries: Vec<QueryRequest>,
/// ) -> Result<Vec<QueryResponse>, TransactionError> {
///     let guard = TransactionGuard::begin_as(executor, RequestContext::default(), false).await?;
///     let mut responses = Vec::new();
///
///     for query in queries {
///         let resp = guard.execute(query).await;
///         if resp.has_errors() {
///             guard.discard().await?;  // Consumes guard, rolls back transaction
///             return Err(TransactionError::execution("query failed"));
///         }
///         responses.push(resp);
///     }
///
///     guard.commit().await?;  // Consumes guard, commits transaction
///     Ok(responses)
/// }
/// ```
#[must_use = "dropping the guard abandons the transaction"]
pub struct TransactionGuard<'a, E: crate::QueryExecutor + ?Sized> {
    executor: &'a E,
    /// The context it was begun in; `None` when begun in the ambient one,
    /// which each later call then reads afresh.
    context: Option<RequestContext>,
    pub(crate) handle: Option<TransactionHandle>,
}

impl<'a, E: crate::QueryExecutor + ?Sized> TransactionGuard<'a, E> {
    /// Begin a new transaction and return a guard for it, in
    /// [`RequestContext::ambient`].
    ///
    /// Dropping the returned guard abandons the transaction.
    pub async fn begin(
        executor: &'a E,
        readonly: bool,
    ) -> std::result::Result<Self, TransactionError> {
        let handle = executor.begin_txn(readonly).await?;
        Ok(Self {
            executor,
            context: None,
            handle: Some(handle),
        })
    }

    /// Begin a new transaction for the request `context` serves and return a
    /// guard for it.
    ///
    /// Dropping the returned guard abandons the transaction.
    pub async fn begin_as(
        executor: &'a E,
        context: RequestContext,
        readonly: bool,
    ) -> std::result::Result<Self, TransactionError> {
        let handle = executor.begin_txn_as(&context, readonly).await?;
        Ok(Self {
            executor,
            context: Some(context),
            handle: Some(handle),
        })
    }

    /// Take ownership of `handle`, a transaction already begun for the
    /// request `context` serves: dropping the returned guard abandons it.
    pub fn adopt(executor: &'a E, context: RequestContext, handle: TransactionHandle) -> Self {
        Self {
            executor,
            context: Some(context),
            handle: Some(handle),
        }
    }

    /// Execute a query within this transaction, in the context it was begun
    /// in.
    pub async fn execute(&self, request: crate::QueryRequest) -> crate::QueryResponse {
        match &self.handle {
            Some(handle) => {
                let request = match &self.context {
                    Some(context) => request.with_context(context.clone()),
                    None => request,
                };
                self.executor.execute_in_txn(request, handle).await
            }
            None => crate::QueryResponse::error("transaction already finalized"),
        }
    }

    /// Get the transaction handle for inspection (e.g., logging).
    pub fn handle(&self) -> Option<&TransactionHandle> {
        self.handle.as_ref()
    }

    /// Commit the transaction and consume the guard.
    ///
    /// After calling this, the guard cannot be used again (compile-time enforced).
    pub async fn commit(mut self) -> std::result::Result<(), TransactionError> {
        let handle = self
            .handle
            .as_ref()
            .ok_or_else(|| TransactionError::already_finalized("transaction already finalized"))?;
        match &self.context {
            Some(context) => self.executor.commit_txn_as(context, handle).await?,
            None => self.executor.commit_txn(handle).await?,
        }
        self.handle = None;
        Ok(())
    }

    /// Discard the transaction and consume the guard.
    ///
    /// After calling this, the guard cannot be used again (compile-time enforced).
    pub async fn discard(mut self) -> std::result::Result<(), TransactionError> {
        let handle = self
            .handle
            .as_ref()
            .ok_or_else(|| TransactionError::already_finalized("transaction already finalized"))?;
        match &self.context {
            Some(context) => self.executor.discard_txn_as(context, handle).await?,
            None => self.executor.discard_txn(handle).await?,
        }
        self.handle = None;
        Ok(())
    }
}

impl<E: crate::QueryExecutor + ?Sized> Drop for TransactionGuard<'_, E> {
    fn drop(&mut self) {
        if let Some(handle) = &self.handle {
            self.executor.abandon_txn(handle);
        }
    }
}
