//! Transaction begin, requests, commit, discard and abandon, each a message
//! to the worker that owns the transaction.

use super::*;
use async_trait::async_trait;
use query::txn::{TransactionHandle, TransactionRegistry, TxnJob};
use storage::corekv::MaybeSend;
use web_time::Instant;

fn report_slow_phase(operation: &'static str, phase: &'static str, started: Instant) {
    let elapsed = started.elapsed();
    if elapsed >= Duration::from_secs(1) {
        tracing::warn!(
            target: "defradb.transaction_phase",
            operation,
            phase,
            elapsed_ms = elapsed.as_millis() as u64,
            "slow native transaction owner phase"
        );
    }
}

/// Runs `job` until it finishes or `timeout` passes, when the job is dropped
/// and the request fails.
async fn within<F: std::future::Future>(
    timeout: Option<Duration>,
    job: F,
) -> std::result::Result<F::Output, TransactionError> {
    query::runner::within_deadline(timeout, job)
        .await
        .map_err(TransactionError::execution)
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl<S: Store + 'static> TransactionRegistry for DbTransactionRegistry<S> {
    fn abandon(&self, handle: &TransactionHandle) {
        if let (Ok(owners), Some(id)) = (self.owners(), TxnId::parse(handle)) {
            owners.abandon(id);
        }
    }

    async fn begin(
        &self,
        readonly: bool,
    ) -> std::result::Result<TransactionHandle, TransactionError> {
        self.begin_as(&RequestContext::ambient(), readonly).await
    }

    async fn begin_as(
        &self,
        context: &RequestContext,
        readonly: bool,
    ) -> std::result::Result<TransactionHandle, TransactionError> {
        let queued = Instant::now();
        let db = self.db.clone();
        let broadcaster = self.broadcaster.clone();
        let opened = self
            .owners()
            .map_err(|error| TransactionError::execution(error.to_string()))?
            .begin(caller(context), move |id| async move {
                report_slow_phase("begin", "queue", queued);
                let started = Instant::now();
                let result = db.new_txn(readonly).await;
                report_slow_phase("begin", "execute", started);
                let mut txn = result?;
                txn.make_explicit();
                OwnedTxn::new(id, txn, db, broadcaster)
            })
            .await
            .map_err(|error| TransactionError::execution(error.to_string()))?;
        let id = opened.map_err(crate::error::transaction_error)?;
        Ok(TransactionHandle::new(id.to_string()))
    }

    async fn run<T>(
        &self,
        handle: &TransactionHandle,
        timeout: Option<Duration>,
        job: TxnJob<T>,
    ) -> std::result::Result<T, TransactionError>
    where
        T: MaybeSend + 'static,
    {
        self.run_as(&RequestContext::ambient(), handle, timeout, job)
            .await
    }

    async fn run_as<T>(
        &self,
        context: &RequestContext,
        handle: &TransactionHandle,
        timeout: Option<Duration>,
        job: TxnJob<T>,
    ) -> std::result::Result<T, TransactionError>
    where
        T: MaybeSend + 'static,
    {
        let id = TxnId::parse(handle).ok_or_else(|| {
            TransactionError::not_found(format!("transaction '{handle}' not found"))
        })?;
        let scope = context.clone();
        let queued = Instant::now();
        self.owners()
            .map_err(|error| unavailable(error, handle))?
            .run(id, &caller(context), move |owned| {
                Box::pin(async move {
                    report_slow_phase("run", "queue", queued);
                    let started = Instant::now();
                    let ran = scope.mirror(within(timeout, job(owned))).await;
                    report_slow_phase("run", "execute", started);
                    owned.after_request();
                    ran
                })
            })
            .await
            .map_err(|error| unavailable(error, handle))?
    }

    async fn read<T>(
        &self,
        timeout: Option<Duration>,
        job: TxnJob<T>,
    ) -> std::result::Result<T, TransactionError>
    where
        T: MaybeSend + 'static,
    {
        self.read_as(&RequestContext::ambient(), timeout, job).await
    }

    async fn read_as<T>(
        &self,
        context: &RequestContext,
        timeout: Option<Duration>,
        job: TxnJob<T>,
    ) -> std::result::Result<T, TransactionError>
    where
        T: MaybeSend + 'static,
    {
        let queued = Instant::now();
        let db = self.db.clone();
        let scope = context.clone();
        self.owners()
            .map_err(|error| TransactionError::execution(error.to_string()))?
            .run_anywhere("txn.read", move || async move {
                report_slow_phase("read", "queue", queued);
                let started = Instant::now();
                let result = scope
                    .mirror(async move {
                        let txn = db
                            .new_txn(true)
                            .await
                            .map_err(crate::error::transaction_error)?;
                        // A snapshot of its own, never registered: no ID names it.
                        let mut owned = OwnedTxn::new(TxnId::unnamed(), txn, db, None)
                            .map_err(|error| TransactionError::execution(error.to_string()))?;
                        within(timeout, job(&mut owned)).await
                    })
                    .await;
                report_slow_phase("read", "execute", started);
                result
            })
            .await
            .map_err(|error| TransactionError::execution(error.to_string()))?
    }

    async fn commit(
        &self,
        handle: &TransactionHandle,
    ) -> std::result::Result<(), TransactionError> {
        self.commit_as(&RequestContext::ambient(), handle).await
    }

    async fn commit_as(
        &self,
        context: &RequestContext,
        handle: &TransactionHandle,
    ) -> std::result::Result<(), TransactionError> {
        let id = TxnId::parse(handle).ok_or_else(|| {
            TransactionError::not_found(format!("transaction '{handle}' not found"))
        })?;
        let label = handle.to_string();
        let queued = Instant::now();
        self.owners()
            .map_err(|error| unavailable(error, handle))?
            .finish(id, &caller(context), move |owned| {
                Box::pin(async move {
                    report_slow_phase("commit", "queue", queued);
                    let started = Instant::now();
                    let result = owned.commit().await;
                    report_slow_phase("commit", "execute", started);
                    result.map_err(|error| commit_error(&label, error))
                })
            })
            .await
            .map_err(|error| unavailable(error, handle))?
    }

    async fn discard(
        &self,
        handle: &TransactionHandle,
    ) -> std::result::Result<(), TransactionError> {
        self.discard_as(&RequestContext::ambient(), handle).await
    }

    async fn discard_as(
        &self,
        context: &RequestContext,
        handle: &TransactionHandle,
    ) -> std::result::Result<(), TransactionError> {
        let id = TxnId::parse(handle).ok_or_else(|| {
            TransactionError::not_found(format!("transaction '{handle}' not found"))
        })?;
        let label = handle.to_string();
        self.owners()
            .map_err(|error| unavailable(error, handle))?
            .finish(id, &caller(context), move |owned| {
                Box::pin(async move {
                    owned.discard().map_err(|error| {
                        TransactionError::execution(format!(
                            "discard error for transaction '{label}': {error}"
                        ))
                    })
                })
            })
            .await
            .map_err(|error| unavailable(error, handle))?
    }
}

/// A commit that lost a write conflict is a typed, retryable conflict.
fn commit_error(handle: &str, error: crate::error::Error) -> TransactionError {
    let message = format!("commit error for transaction '{handle}': {error}");
    if error.is_txn_conflict() {
        TransactionError::conflict(message)
    } else {
        match crate::error::transaction_error(error) {
            TransactionError::Storage(storage::Error::Io(cause)) => {
                TransactionError::Storage(storage::Error::Io(
                    cause.with_context(format!("commit error for transaction '{handle}'")),
                ))
            }
            _ => TransactionError::execution(message),
        }
    }
}
