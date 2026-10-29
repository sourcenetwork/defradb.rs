//! Single-flight ownership of a CID for its fetch and its ingestion.
//!
//! The first caller to claim a CID owns its fetch or ingest; a duplicate
//! learns at once that someone else does, and either backs off or waits on
//! the owner's [`Event`], which is set when the owner lets go. Merges claim
//! nothing here: one merge per CID comes from the exactly-once race on the
//! block's applied key (`h/p/`) in the merge transaction.

use cid::Cid;
use kovan_map::HopscotchMap;
use rapidhash::fast::RandomState;
use regolith::sync::{Event, Notify};
use std::sync::Arc;

/// The CIDs whose fetch or ingest is owned right now.
///
/// # Go Compatibility
///
/// Go's `processQueue` (`p2p.go:565-611`) also serializes merges; here the
/// merge transaction settles that race instead.
#[derive(Clone)]
pub struct ProcessQueue {
    inner: Arc<ProcessQueueInner>,
}

impl std::fmt::Debug for ProcessQueue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProcessQueue").finish_non_exhaustive()
    }
}

struct ProcessQueueInner {
    /// Wakes the pending-DAG dispatch owner whenever a CID is let go.
    released: Notify,
    /// Each owned CID with the event its owner sets when it lets go.
    owners: HopscotchMap<Cid, Arc<Event>, RandomState>,
}

/// A wait for the current owner of a CID to let it go.
#[derive(Debug)]
pub struct ProcessWait {
    event: Arc<Event>,
}

impl ProcessWait {
    /// Completes once the owner lets the CID go, at once if it already has.
    pub async fn released(self) {
        self.event.wait().await;
    }
}

impl ProcessQueue {
    /// Create a new process queue.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(ProcessQueueInner {
                released: Notify::new(),
                owners: HopscotchMap::with_hasher(RandomState::default()),
            }),
        }
    }

    /// Get the number of CIDs currently being processed.
    ///
    /// Useful for monitoring and debugging.
    pub fn active_count(&self) -> usize {
        self.inner.owners.len()
    }

    pub(crate) fn is_active(&self, cid: &Cid) -> bool {
        self.inner.owners.contains_key(cid)
    }

    /// Completes after some CID is let go: at the next release, or at once
    /// when one happened since the last call found nobody waiting.
    pub(crate) async fn released(&self) {
        self.inner.released.notified().await;
    }

    /// Get all CIDs currently being processed.
    ///
    /// Useful for debugging stuck operations.
    pub fn active_cids(&self) -> Vec<Cid> {
        self.inner.owners.keys().collect()
    }

    /// Force release a stuck CID.
    ///
    /// # Safety
    ///
    /// Only call this for CIDs that you are certain are stuck. Calling this
    /// while a legitimate processing operation is in progress may cause
    /// duplicate processing.
    ///
    /// # Returns
    ///
    /// Returns `true` if the CID was released, `false` if it wasn't locked.
    pub fn force_release(&self, cid: &Cid) -> bool {
        if !self.release_sync(cid) {
            return false;
        }
        tracing::warn!(?cid, "Force-releasing stuck CID");
        true
    }

    /// Force release all stuck CIDs.
    ///
    /// Use this during cleanup or recovery to release all locked CIDs.
    ///
    /// # Returns
    ///
    /// Returns the number of CIDs that were released.
    pub fn force_release_all(&self) -> usize {
        let cids: Vec<Cid> = self.inner.owners.keys().collect();
        let count = cids.iter().filter(|cid| self.release_sync(cid)).count();
        if count > 0 {
            tracing::warn!(count = count, "Force-releasing all stuck CIDs");
        }
        count
    }

    /// Claim a CID, or learn who owns it.
    ///
    /// Returns:
    /// - `Ok(ProcessGuard)` if this caller owns the CID now
    /// - `Err(ProcessWait)` if another caller does; await
    ///   [`ProcessWait::released`] to know when it lets go
    ///
    /// # Example
    ///
    /// ```ignore
    /// let queue = ProcessQueue::new();
    ///
    /// match queue.try_acquire(&cid) {
    ///     Ok(guard) => {
    ///         fetch(&cid).await;
    ///         // Dropping the guard lets the CID go and wakes its waiters.
    ///     }
    ///     Err(wait) => wait.released().await,
    /// }
    /// ```
    pub fn try_acquire(&self, cid: &Cid) -> Result<ProcessGuard, ProcessWait> {
        match self
            .inner
            .owners
            .insert_if_absent(*cid, Arc::new(Event::new()))
        {
            None => Ok(ProcessGuard {
                cid: *cid,
                queue: self.clone(),
            }),
            Some(event) => Err(ProcessWait { event }),
        }
    }

    /// Claim a CID, or return `None` at once when another caller owns it.
    pub fn try_acquire_nowait(&self, cid: &Cid) -> Option<ProcessGuard> {
        self.try_acquire(cid).ok()
    }

    /// Try to acquire exclusive processing rights for every CID without
    /// registering a waiter.
    ///
    /// CAR responses can contain blocks shared by several document DAGs.  A
    /// root-only guard therefore does not prevent two imports from racing the
    /// same mutable merge marker. Sorting and de-duplicating the keys gives one
    /// batch every affected CID in canonical order; the first CID that already
    /// has an owner releases everything acquired so far and is returned, so no
    /// partial ownership outlives the call. This is intentionally
    /// non-waiting: duplicate CAR arrivals must not retain global transport
    /// task slots while the owner they are waiting for needs that transport to
    /// make progress.
    pub(crate) fn try_acquire_all_nowait<I>(&self, cids: I) -> Result<Vec<ProcessGuard>, Cid>
    where
        I: IntoIterator<Item = Cid>,
    {
        let mut cids: Vec<_> = cids.into_iter().collect();
        cids.sort_unstable();
        cids.dedup();

        let mut guards = Vec::with_capacity(cids.len());
        for cid in cids {
            match self.try_acquire_nowait(&cid) {
                Some(guard) => guards.push(guard),
                None => return Err(cid),
            }
        }
        Ok(guards)
    }

    /// Let the CID go: its waiters wake and the dispatch owner is told.
    /// Returns whether the CID was owned.
    fn release_sync(&self, cid: &Cid) -> bool {
        let Some(event) = self.inner.owners.force_remove(cid) else {
            return false;
        };
        event.set();
        self.inner.released.notify_one();
        true
    }
}

impl Default for ProcessQueue {
    fn default() -> Self {
        Self::new()
    }
}

/// Guard that releases the CID from the queue when dropped.
#[derive(Debug)]
pub struct ProcessGuard {
    cid: Cid,
    queue: ProcessQueue,
}

impl ProcessGuard {
    /// Get the CID being processed.
    pub fn cid(&self) -> &Cid {
        &self.cid
    }

    /// Explicitly release the guard.
    ///
    /// This is the preferred way to release as it provides explicit control.
    /// The guard will also be released automatically on drop.
    pub async fn release(self) {
        self.queue.release_sync(&self.cid);
        // Prevent Drop from running
        std::mem::forget(self);
    }
}

impl Drop for ProcessGuard {
    fn drop(&mut self) {
        self.queue.release_sync(&self.cid);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::future::Future;
    use std::pin::pin;
    use std::str::FromStr;
    use std::task::{Context, Poll, Waker};
    use std::time::Duration;

    fn test_cid() -> Cid {
        Cid::from_str("bafybeigdyrzt5sfp7udm7hu76uh7y26nf3efuylqabf3oclgtqy55fbzdi").unwrap()
    }

    fn test_cid2() -> Cid {
        Cid::from_str("bafkreigh2akiscaildcqabsyg3dfr6chu3fgpregiymsck7e7aqa4s52zy").unwrap()
    }

    #[test]
    fn test_first_caller_acquires() {
        let queue = ProcessQueue::new();
        assert!(
            queue.try_acquire(&test_cid()).is_ok(),
            "First caller should acquire"
        );
    }

    #[test]
    fn test_second_caller_waits() {
        let queue = ProcessQueue::new();
        let cid = test_cid();
        let _guard = queue.try_acquire(&cid).unwrap();
        assert!(
            queue.try_acquire(&cid).is_err(),
            "Second caller should wait"
        );
    }

    #[test]
    fn test_nowait_second_caller_is_suppressed() {
        let queue = ProcessQueue::new();
        let cid = test_cid();

        let _guard = queue.try_acquire_nowait(&cid).unwrap();

        assert!(queue.try_acquire_nowait(&cid).is_none());
    }

    #[test]
    fn test_different_cids_independent() {
        let queue = ProcessQueue::new();
        let _guard1 = queue.try_acquire(&test_cid()).unwrap();
        assert!(
            queue.try_acquire(&test_cid2()).is_ok(),
            "Different CIDs should be independent"
        );
    }

    #[test]
    fn acquire_all_nowait_is_atomic_for_an_overlapping_batch() {
        let queue = ProcessQueue::new();
        let cid1 = test_cid();
        let cid2 = test_cid2();

        let mut ordered = [cid1, cid2];
        ordered.sort_unstable();
        let blocker = queue.try_acquire_nowait(&ordered[1]).unwrap();
        assert_eq!(
            queue
                .try_acquire_all_nowait([cid2, cid1, cid2])
                .unwrap_err(),
            ordered[1]
        );
        assert_eq!(queue.active_count(), 1);
        let unblocked = queue
            .try_acquire_nowait(&ordered[0])
            .expect("failed batch must not transiently retain its earlier CID");
        drop(unblocked);
        drop(blocker);

        let second = queue.try_acquire_all_nowait([cid1, cid2]).unwrap();
        assert_eq!(second.len(), 2);
        drop(second);
        assert_eq!(queue.active_count(), 0);
    }

    /// A wait is the owner's event: it completes when the owner lets go,
    /// with no runtime, and a wait on an owner already gone completes at once.
    #[test]
    fn a_wait_completes_when_its_owner_lets_go_without_a_runtime() {
        let queue = ProcessQueue::new();
        let cid = test_cid();
        let guard = queue.try_acquire(&cid).unwrap();
        let first = queue.try_acquire(&cid).unwrap_err();
        let second = queue.try_acquire(&cid).unwrap_err();

        let mut cx = Context::from_waker(Waker::noop());
        let mut waiting = pin!(first.released());
        assert!(waiting.as_mut().poll(&mut cx).is_pending());

        drop(guard);
        assert!(waiting.as_mut().poll(&mut cx).is_ready());
        assert_eq!(
            pin!(second.released()).poll(&mut cx),
            Poll::Ready(()),
            "a wait taken before the release and polled after it completes at once"
        );
        assert!(queue.try_acquire(&cid).is_ok());
    }

    #[nuclei::test]
    async fn test_waiter_notified_on_release() {
        let queue = ProcessQueue::new();
        let cid = test_cid();

        let guard = queue.try_acquire(&cid).unwrap();
        let wait = queue.try_acquire(&cid).unwrap_err();
        let waiter = crate::rt::spawn(async move {
            wait.released().await;
            true
        });

        guard.release().await;

        let result = crate::rt::time::timeout(Duration::from_secs(5), waiter.join()).await;
        assert!(result.is_ok(), "Waiter should be notified");
        assert!(
            result.unwrap().unwrap(),
            "Waiter should complete successfully"
        );
    }

    #[nuclei::test]
    async fn test_multiple_waiters_all_notified() {
        let queue = ProcessQueue::new();
        let cid = test_cid();

        let guard = queue.try_acquire(&cid).unwrap();
        let handles: Vec<_> = (0..5)
            .map(|_| {
                let wait = queue.try_acquire(&cid).unwrap_err();
                crate::rt::spawn(wait.released())
            })
            .collect();

        guard.release().await;

        for handle in handles {
            let result = crate::rt::time::timeout(Duration::from_secs(5), handle).await;
            assert!(result.is_ok(), "All waiters should be notified");
        }
    }

    #[test]
    fn test_reacquire_after_release() {
        let queue = ProcessQueue::new();
        let cid = test_cid();
        drop(queue.try_acquire(&cid).unwrap());
        assert!(
            queue.try_acquire(&cid).is_ok(),
            "Should be able to reacquire after release"
        );
    }

    #[test]
    fn test_cancelled_waiters_handled_gracefully() {
        let queue = ProcessQueue::new();
        let cid = test_cid();

        let guard = queue.try_acquire(&cid).unwrap();
        drop(queue.try_acquire(&cid).unwrap_err());
        let mut cx = Context::from_waker(Waker::noop());
        {
            // A wait polled once and then cancelled.
            let mut cancelled = pin!(queue.try_acquire(&cid).unwrap_err().released());
            assert!(cancelled.as_mut().poll(&mut cx).is_pending());
        }

        drop(guard);

        assert!(
            queue.try_acquire(&cid).is_ok(),
            "Queue should be clean after handling cancelled waiters"
        );
    }

    #[test]
    fn a_release_wakes_the_dispatch_owner_once() {
        let queue = ProcessQueue::new();
        let cid = test_cid();
        let mut cx = Context::from_waker(Waker::noop());
        let mut released = pin!(queue.released());
        assert!(released.as_mut().poll(&mut cx).is_pending());

        drop(queue.try_acquire(&cid).unwrap());

        assert!(released.as_mut().poll(&mut cx).is_ready());
    }

    #[test]
    fn force_release_sets_the_owner_event() {
        let queue = ProcessQueue::new();
        let cid = test_cid();
        let _stuck = queue.try_acquire(&cid).unwrap();
        let wait = queue.try_acquire(&cid).unwrap_err();

        assert!(queue.force_release(&cid));
        assert!(!queue.force_release(&cid));

        let mut cx = Context::from_waker(Waker::noop());
        assert!(pin!(wait.released()).poll(&mut cx).is_ready());
        assert_eq!(queue.active_count(), 0);
    }
}
