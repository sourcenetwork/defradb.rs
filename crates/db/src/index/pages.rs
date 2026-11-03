//! Index work done one page per engine transaction.
//!
//! A job that scales with a collection (an index backfill, a reindex) runs
//! as a chain of pages, each its own `transact_retry`, so no transaction
//! holds more than a page. A page does at most `size` items of work; a
//! conflict re-runs it at half the size, down to one item, so a stream of
//! writes into the range a page covers slows the job rather than starving
//! it, and a commit doubles the size back up to [`PAGE_ITEMS`]. A page
//! writes whatever the job resumes from in its own transaction, so a restart
//! picks up after the last page that committed.

use async_trait::async_trait;
use datastore::BasicTxn;
use defra_core::thread_bounds::MaybeSend;
use storage::corekv::{
    RetryAttemptError, RetryAttemptFn, RetryAttemptFuture, RetryAttemptValue, RetryOptions,
    RetryTxn, Store,
};

use crate::error::{Error, Result};
use crate::DB;

/// Items one page does at most. A document adds a handful of index keys to
/// the write set, so a page, and with it the window in which a concurrent
/// write to one of its documents conflicts, stays well under a millisecond
/// for an ordered index.
pub const PAGE_ITEMS: usize = 256;

/// One page of a paged job.
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub(crate) trait Page: MaybeSend + 'static {
    /// What a committed page reports.
    type Output: MaybeSend + 'static;

    /// Does at most `size` items of the job's work in `txn`. It may run more
    /// than once, each time against a fresh attempt.
    async fn run(&mut self, txn: &mut BasicTxn, size: usize) -> Result<Self::Output>;
}

impl<S> DB<S>
where
    S: Store,
{
    /// Commits one page of at most `*size` items and sets `*size` to the
    /// size the next page gets.
    pub(crate) async fn run_page<P>(&self, page: P, size: &mut usize) -> Result<P::Output>
    where
        P: Page,
    {
        let first = (*size).max(1);
        // Each conflict halves the page, so the policy's budget is extended
        // by the halvings that can bring it down to one item.
        let attempts = self
            .options()
            .retry_options()
            .max_attempts
            .saturating_add(first.ilog2());
        let attempt = PageAttempt {
            page,
            size: first,
            crdts: self.crdts().clone(),
        };
        match self
            .store()
            .transact_retry(RetryOptions::new(attempts), Box::new(attempt))
            .await
        {
            Ok(value) => {
                let (output, committed) = *value
                    .downcast::<(P::Output, usize)>()
                    .map_err(|_| Error::Other("invalid index page result".into()))?;
                *size = (committed * 2).min(PAGE_ITEMS);
                Ok(output)
            }
            Err(error) => Err(match error.downcast_abort::<Error>() {
                Ok(error) => error,
                Err(error) => Error::Storage(error.into_storage()),
            }),
        }
    }
}

struct PageAttempt<P> {
    page: P,
    size: usize,
    crdts: crdt::Crdts,
}

impl<P> RetryAttemptFn for PageAttempt<P>
where
    P: Page,
{
    fn run<'a>(&'a mut self, retry: &'a mut dyn RetryTxn) -> RetryAttemptFuture<'a> {
        Box::pin(async move {
            if let Some(reason) = retry.previous_conflict() {
                self.size = (self.size / 2).max(1);
                tracing::debug!(
                    access = ?reason.access,
                    winning_write = ?reason.winning_write,
                    size = self.size,
                    "shrinking a conflicted index page"
                );
            }
            let io = retry.begin_attempt_io()?;
            let mut txn = BasicTxn::from_retry_attempt_io(io, 0, self.crdts.clone());
            let output = self
                .page
                .run(&mut txn, self.size)
                .await
                .map_err(|error| RetryAttemptError::Abort(Box::new(error)))?;
            let (io, callbacks) = txn.into_retry_attempt()?;
            retry.finish_attempt_io(io, callbacks)?;
            Ok(Box::new((output, self.size)) as RetryAttemptValue)
        })
    }
}
