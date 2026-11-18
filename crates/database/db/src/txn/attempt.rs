//! One engine-retried write as a closure over a fresh [`DbTxn`]: the closure
//! runs once per attempt, and a conflict runs it again on fresh reads.

use std::sync::Arc;

use defra_core::thread_bounds::{MaybeBoxFuture, MaybeSend};
use storage::corekv::{
    RetryAttemptError, RetryAttemptFn, RetryAttemptFuture, RetryAttemptValue, RetryTxn, Store,
};

use crate::txn::DbTxn;
use crate::{Error, Result};

struct DbAttempt<S, F>
where
    S: Store,
{
    db: Arc<crate::database::DB<S>>,
    run: F,
}

impl<S, F, T> RetryAttemptFn for DbAttempt<S, F>
where
    S: Store + 'static,
    F: MaybeSend + for<'t> FnMut(&'t DbTxn<S>) -> MaybeBoxFuture<'t, Result<T>>,
    T: Send + 'static,
{
    fn run<'a>(&'a mut self, retry: &'a mut dyn RetryTxn) -> RetryAttemptFuture<'a> {
        Box::pin(async move {
            let io = retry.begin_attempt_io()?;
            let txn = self
                .db
                .new_db_txn(datastore::BasicTxn::from_retry_attempt_io(
                    io,
                    0,
                    self.db.crdts().clone(),
                ));
            let value = (self.run)(&txn)
                .await
                .map_err(|error| RetryAttemptError::Abort(Box::new(error)))?;
            let (io, callbacks) = txn.into_retry_attempt()?;
            retry.finish_attempt_io(io, callbacks)?;
            Ok(Box::new(value) as RetryAttemptValue)
        })
    }
}

impl<S> crate::database::DB<S>
where
    S: Store + 'static,
{
    /// Run `run` in an engine-retried transaction under the node's retry
    /// policy, and return what the attempt that committed returned.
    pub(crate) async fn transact_db<T, F>(self: &Arc<Self>, run: F) -> Result<T>
    where
        F: MaybeSend + 'static + for<'t> FnMut(&'t DbTxn<S>) -> MaybeBoxFuture<'t, Result<T>>,
        T: Send + 'static,
    {
        let attempt = DbAttempt {
            db: Arc::clone(self),
            run,
        };
        let value = self
            .store()
            .transact_retry(self.options().retry_options(), Box::new(attempt))
            .await
            .map_err(|error| match error.downcast_abort::<Error>() {
                Ok(error) => error,
                Err(error) => Error::Storage(error.into_storage()),
            })?;
        value
            .downcast::<T>()
            .map(|value| *value)
            .map_err(|_| Error::Other("invalid retried attempt result".into()))
    }
}
