//! Engine transactions over the namespaced datastore facade.
//!
//! [`transact`] runs a body in one engine transaction of a store through
//! [`storage::corekv::transact`]: each run gets a fresh [`BasicTxn`] over the
//! attempt, lent mutably so the body's future stays `Send`, and the body
//! reads and writes through the same namespace views as any other datastore
//! code; the store commits the run that wins. The
//! body may run more than once, so anything with an effect outside the
//! transaction is prepared before it.
//!
//! ```ignore
//! let added: usize = datastore::transact(&store, retry, move |txn| {
//!     Box::pin(mark(txn, keys.clone()))
//! })
//! .await?;
//! ```

use std::any::Any;

use storage::corekv::{
    MaybeSend, RetryAttemptError, RetryAttemptFn, RetryAttemptFuture, RetryOptions, RetryTxn, Store,
};

use crate::BasicTxn;

/// Runs `body` in one engine transaction of `store`, re-running it after a
/// conflict under `options`, and returns the value of the run that committed.
/// The body's value is boxed as a `RetryAttemptValue` of type `T`.
pub async fn transact<S, T, F>(
    store: &S,
    options: RetryOptions,
    body: F,
) -> Result<T, RetryAttemptError>
where
    S: Store + ?Sized,
    T: Any,
    F: for<'t> FnMut(&'t mut BasicTxn) -> RetryAttemptFuture<'t> + MaybeSend + 'static,
{
    let crdts = store.crdts().clone();
    storage::corekv::transact(
        store,
        options,
        None,
        Box::new(FacadeAttempt { body, crdts }),
    )
    .await
}

/// One attempt: the body over a facade of the attempt's transaction, whose
/// writes and callbacks go back to the store for its commit.
struct FacadeAttempt<F> {
    body: F,
    crdts: storage::Crdts,
}

impl<F> RetryAttemptFn for FacadeAttempt<F>
where
    F: for<'t> FnMut(&'t mut BasicTxn) -> RetryAttemptFuture<'t> + MaybeSend,
{
    fn run<'a>(&'a mut self, retry: &'a mut dyn RetryTxn) -> RetryAttemptFuture<'a> {
        Box::pin(async move {
            let io = retry.begin_attempt_io()?;
            let mut txn = BasicTxn::from_retry_attempt_io(io, 0, self.crdts.clone());
            let value = (self.body)(&mut txn).await?;
            let (io, callbacks) = txn.into_retry_attempt()?;
            retry.finish_attempt_io(io, callbacks)?;
            Ok(value)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use storage::corekv::{engine_retry_options, RetryAttemptValue};
    use storage::RegolithStore;

    async fn count_marks(txn: &mut BasicTxn, keys: Vec<Vec<u8>>) -> RetryAttemptResult {
        let systemstore = txn.systemstore();
        let mut marked = 0usize;
        for key in &keys {
            if systemstore.get(key).await?.is_none() {
                systemstore.set(key, b"marked").await?;
                marked += 1;
            }
        }
        Ok(Box::new(marked) as RetryAttemptValue)
    }

    type RetryAttemptResult = Result<RetryAttemptValue, RetryAttemptError>;

    #[test]
    fn the_body_commits_through_the_facade_and_returns_its_value() {
        workers::block_on(async {
            let store = Arc::new(RegolithStore::in_memory().unwrap());
            let keys = vec![b"a".to_vec(), b"b".to_vec()];

            let first = keys.clone();
            let marked: usize = transact(&*store, engine_retry_options(1), move |txn| {
                Box::pin(count_marks(txn, first.clone()))
            })
            .await
            .unwrap();
            assert_eq!(marked, 2);

            let again: usize = transact(&*store, engine_retry_options(1), move |txn| {
                Box::pin(count_marks(txn, keys.clone()))
            })
            .await
            .unwrap();
            assert_eq!(again, 0, "the first transaction's writes committed");
        })
        .unwrap()
    }

    #[test]
    fn an_error_from_the_body_ends_the_transaction_with_nothing_written() {
        workers::block_on(async {
            let store = Arc::new(RegolithStore::in_memory().unwrap());
            let error = transact::<_, usize, _>(&*store, engine_retry_options(1), |txn| {
                Box::pin(async move {
                    txn.systemstore().set(b"a", b"never").await?;
                    Err(RetryAttemptError::Abort(Box::new("refused")))
                })
            })
            .await
            .unwrap_err();
            assert!(matches!(error, RetryAttemptError::Abort(_)));

            let marked: usize = transact(&*store, engine_retry_options(1), |txn| {
                Box::pin(count_marks(txn, vec![b"a".to_vec()]))
            })
            .await
            .unwrap();
            assert_eq!(marked, 1, "the refused body wrote nothing");
        })
        .unwrap()
    }
}
