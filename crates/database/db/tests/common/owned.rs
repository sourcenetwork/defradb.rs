//! Running test work on an explicit transaction where it lives: on the
//! worker that owns it, as one request, with the transaction lent to the
//! work's handles.

use std::sync::Arc;

use db::DbTransactionRegistry;
use defra_core::thread_bounds::MaybeBoxFuture;
use query::txn::{txn_job, TransactionContext, TransactionRegistry};
use query::TransactionHandle;
use storage::corekv::Store;

/// Runs `work` on the transaction `handle` names, with its context.
pub async fn in_txn<S, T, W>(
    registry: &DbTransactionRegistry<S>,
    handle: &TransactionHandle,
    work: W,
) -> T
where
    S: Store + 'static,
    T: Send + 'static,
    W: FnOnce(Arc<dyn TransactionContext>) -> MaybeBoxFuture<'static, T> + Send + 'static,
{
    registry
        .run(
            handle,
            None,
            txn_job(move |turn| {
                Box::pin(async move {
                    let context = turn.context().expect("the owner lends the transaction");
                    work(context).await
                })
            }),
        )
        .await
        .expect("the transaction's owner runs the work")
}
