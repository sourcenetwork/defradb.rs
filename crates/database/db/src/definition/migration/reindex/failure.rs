//! Recording why an owed reindex stopped, so the index listing shows it.
//!
//! A rebuild a migration's unique clash stops writes the index and the reason
//! onto the owed record of the token it ran under
//! (`proofs/tla/ReindexFailureListing.tla`). A reindex owed again starts
//! without one, and the finishing rebuild removes it with the record.

use std::sync::Arc;

use storage::corekv::Store;

use crate::collection::epoch::EpochedCollections;
use crate::error::Error;
use crate::DB;

impl<S> DB<S>
where
    S: Store,
{
    /// Records on the reindex owed under `token` the unique index a
    /// migration left `error` unable to fill, so the index lists as failed
    /// until the reindex finishes. Any other error records nothing. The
    /// record only shows the failure, so one that cannot be written is
    /// reported and the rebuild's own error stands.
    pub(super) async fn record_failure(&self, collection_id: &str, token: u64, error: &Error) {
        let Error::ReindexUniqueViolation { index_id, message } = error else {
            return;
        };
        let attempt = RecordFailureAttempt {
            collection_id: collection_id.to_owned(),
            token,
            index: *index_id,
            reason: message.clone(),
            collections: Arc::clone(&self.collections),
            crdts: self.crdts().clone(),
        };
        if let Err(record_error) = self
            .store()
            .transact_retry(self.options().retry_options(), Box::new(attempt))
            .await
            .map_err(|error| match error.downcast_abort::<Error>() {
                Ok(error) => error,
                Err(error) => Error::Storage(error.into_storage()),
            })
        {
            tracing::warn!(
                %collection_id,
                error = %record_error,
                "the failure of a reindex could not be recorded"
            );
        }
    }
}

/// Writes a stopped rebuild's failure onto the owed record it ran under.
struct RecordFailureAttempt {
    collection_id: String,
    token: u64,
    index: u32,
    reason: String,
    collections: Arc<EpochedCollections>,
    crdts: storage::Crdts,
}

impl storage::corekv::RetryAttemptFn for RecordFailureAttempt {
    fn run<'a>(
        &'a mut self,
        retry: &'a mut dyn storage::corekv::RetryTxn,
    ) -> storage::corekv::RetryAttemptFuture<'a> {
        Box::pin(async move {
            let io = retry.begin_attempt_io()?;
            let txn = datastore::BasicTxn::from_retry_attempt_io(io, 0, self.crdts.clone());
            let systemstore = txn.systemstore();
            let recorded = super::owed::record_failure(
                &systemstore,
                &self.collections,
                &self.collection_id,
                self.token,
                self.index,
                self.reason.clone(),
            )
            .await;
            recorded.map_err(|error| storage::corekv::RetryAttemptError::Abort(Box::new(error)))?;
            drop(systemstore);
            let (io, callbacks) = txn.into_retry_attempt()?;
            retry.finish_attempt_io(io, callbacks)?;
            Ok(Box::new(()) as storage::corekv::RetryAttemptValue)
        })
    }
}
