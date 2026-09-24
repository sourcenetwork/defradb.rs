//! Process queue for serializing concurrent sync operations.
//!
//! This matches Go's `processQueue` in `p2p.go` which prevents multiple
//! goroutines from syncing the same CID concurrently, avoiding transaction
//! conflicts during merge.

use cid::Cid;
use kovan_map::HopscotchMap;
use kovan_queue::seg_queue::SegQueue;
use rapidhash::fast::RandomState;
use std::sync::atomic::{fence, AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::oneshot;

/// A queue that serializes processing of the same CID.
///
/// When multiple sync requests arrive for the same CID concurrently,
/// only the first one proceeds while others wait. Once the first
/// completes, all waiters are released (they can then check if the
/// block is already merged and skip processing).
///
/// # Go Compatibility
///
/// This matches Go's `processQueue` pattern in `p2p.go:565-611`.
#[derive(Clone)]
pub struct ProcessQueue {
    inner: Arc<ProcessQueueInner>,
}

impl std::fmt::Debug for ProcessQueue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProcessQueue").finish_non_exhaustive()
    }
}

#[derive(Default)]
struct Waiters {
    senders: SegQueue<oneshot::Sender<()>>,
    released: AtomicBool,
}

impl Waiters {
    /// Marks the entry released before draining, with a SeqCst fence on both
    /// this side and the joining side, so a sender pushed after the drain
    /// always observes the flag and re-elects instead of waiting forever.
    fn release(&self) -> (usize, usize) {
        self.released.store(true, Ordering::Relaxed);
        fence(Ordering::SeqCst);
        let mut total = 0;
        let mut notified = 0;
        while let Some(tx) = self.senders.pop() {
            total += 1;
            if tx.send(()).is_ok() {
                notified += 1;
            }
        }
        (notified, total)
    }

    fn join(&self) -> Option<oneshot::Receiver<()>> {
        let (tx, rx) = oneshot::channel();
        self.senders.push(tx);
        fence(Ordering::SeqCst);
        (!self.released.load(Ordering::Relaxed)).then_some(rx)
    }
}

struct ProcessQueueInner {
    released: tokio::sync::Notify,
    waiters: HopscotchMap<Cid, Arc<Waiters>, RandomState>,
}

impl ProcessQueue {
    /// Create a new process queue.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(ProcessQueueInner {
                released: tokio::sync::Notify::new(),
                waiters: HopscotchMap::with_hasher(RandomState::default()),
            }),
        }
    }

    /// Get the number of CIDs currently being processed.
    ///
    /// Useful for monitoring and debugging.
    pub fn active_count(&self) -> usize {
        self.inner.waiters.len()
    }

    pub(crate) fn is_active(&self, cid: &Cid) -> bool {
        self.inner.waiters.contains_key(cid)
    }

    pub(crate) async fn released(&self) {
        self.inner.released.notified().await;
    }

    /// Get all CIDs currently being processed.
    ///
    /// Useful for debugging stuck operations.
    pub fn active_cids(&self) -> Vec<Cid> {
        self.inner.waiters.keys().collect()
    }

    /// Force release a stuck CID.
    ///
    /// Use this to recover from situations where a ProcessGuard was dropped
    /// outside a tokio runtime and the CID became permanently locked.
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
        let Some(waiting) = self.inner.waiters.force_remove(cid) else {
            return false;
        };
        self.inner.released.notify_one();
        tracing::warn!(
            ?cid,
            waiter_count = waiting.senders.len(),
            "Force-releasing stuck CID"
        );
        waiting.release();
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
        let cids: Vec<Cid> = self.inner.waiters.keys().collect();
        let mut count = 0;
        for cid in cids {
            let Some(waiting) = self.inner.waiters.force_remove(&cid) else {
                continue;
            };
            count += 1;
            tracing::debug!(?cid, "Force-releasing CID");
            waiting.release();
        }
        if count > 0 {
            self.inner.released.notify_one();
            tracing::warn!(count = count, "Force-releasing all stuck CIDs");
        }
        count
    }

    /// Try to acquire exclusive processing rights for a CID.
    ///
    /// Returns:
    /// - `Ok(ProcessGuard)` if this caller should process the CID
    /// - `Err(receiver)` if another caller is already processing; wait on the receiver
    ///
    /// # Example
    ///
    /// ```ignore
    /// let queue = ProcessQueue::new();
    ///
    /// match queue.try_acquire(&cid).await {
    ///     Ok(guard) => {
    ///         // We're the first - do the sync work
    ///         do_sync(&cid).await;
    ///         // Guard drop notifies waiters
    ///     }
    ///     Err(rx) => {
    ///         // Another task is processing - wait for it
    ///         let _ = rx.await;
    ///         // Now check if block is merged and proceed accordingly
    ///     }
    /// }
    /// ```
    pub async fn try_acquire(&self, cid: &Cid) -> Result<ProcessGuard, oneshot::Receiver<()>> {
        loop {
            let existing = match self.inner.waiters.get(cid) {
                Some(existing) => existing,
                None => match self
                    .inner
                    .waiters
                    .insert_if_absent(*cid, Arc::new(Waiters::default()))
                {
                    None => {
                        return Ok(ProcessGuard {
                            cid: *cid,
                            queue: self.clone(),
                        })
                    }
                    Some(existing) => existing,
                },
            };
            if let Some(rx) = existing.join() {
                return Err(rx);
            }
        }
    }

    /// Try to acquire a CID without allocating or registering a waiter.
    ///
    /// Returns `None` immediately when another caller owns the CID.
    pub fn try_acquire_nowait(&self, cid: &Cid) -> Option<ProcessGuard> {
        self.inner
            .waiters
            .insert_if_absent(*cid, Arc::new(Waiters::default()))
            .is_none()
            .then(|| ProcessGuard {
                cid: *cid,
                queue: self.clone(),
            })
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

    /// Release the CID and notify all waiters (synchronous version).
    fn release_sync(&self, cid: &Cid) {
        let Some(waiting) = self.inner.waiters.force_remove(cid) else {
            return;
        };
        self.inner.released.notify_one();
        let (notified, total) = waiting.release();
        if notified < total {
            tracing::debug!(
                ?cid,
                notified,
                total,
                "Some waiters were cancelled before notification"
            );
        }
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
        // Release synchronously so it works both inside and outside a runtime.
        self.queue.release_sync(&self.cid);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;
    use std::time::Duration;

    fn test_cid() -> Cid {
        Cid::from_str("bafybeigdyrzt5sfp7udm7hu76uh7y26nf3efuylqabf3oclgtqy55fbzdi").unwrap()
    }

    fn test_cid2() -> Cid {
        Cid::from_str("bafkreigh2akiscaildcqabsyg3dfr6chu3fgpregiymsck7e7aqa4s52zy").unwrap()
    }

    #[tokio::test]
    async fn test_first_caller_acquires() {
        let queue = ProcessQueue::new();
        let cid = test_cid();

        let result = queue.try_acquire(&cid).await;
        assert!(result.is_ok(), "First caller should acquire");
    }

    #[tokio::test]
    async fn test_second_caller_waits() {
        let queue = ProcessQueue::new();
        let cid = test_cid();

        // First caller acquires
        let _guard = queue.try_acquire(&cid).await.unwrap();

        // Second caller should get a waiter
        let result = queue.try_acquire(&cid).await;
        assert!(result.is_err(), "Second caller should wait");
    }

    #[tokio::test]
    async fn test_nowait_second_caller_is_suppressed() {
        let queue = ProcessQueue::new();
        let cid = test_cid();

        let _guard = queue.try_acquire_nowait(&cid).unwrap();

        assert!(queue.try_acquire_nowait(&cid).is_none());
    }

    #[tokio::test]
    async fn test_different_cids_independent() {
        let queue = ProcessQueue::new();
        let cid1 = test_cid();
        let cid2 = test_cid2();

        // First CID acquired
        let _guard1 = queue.try_acquire(&cid1).await.unwrap();

        // Second CID should also acquire (different CID)
        let result = queue.try_acquire(&cid2).await;
        assert!(result.is_ok(), "Different CIDs should be independent");
    }

    #[tokio::test]
    async fn acquire_all_nowait_is_atomic_for_an_overlapping_batch() {
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

    #[tokio::test]
    async fn test_waiter_notified_on_release() {
        let queue = ProcessQueue::new();
        let cid = test_cid();

        // First caller acquires
        let guard = queue.try_acquire(&cid).await.unwrap();

        // Spawn second caller that waits
        let queue_clone = queue.clone();
        let waiter = n0_future::task::spawn(async move {
            let rx = queue_clone.try_acquire(&cid).await.unwrap_err();
            // Wait for notification
            rx.await.unwrap();
            true
        });

        // Give the waiter time to register
        n0_future::time::sleep(Duration::from_millis(10)).await;

        // Explicitly release guard
        guard.release().await;

        // Waiter should complete
        let result = n0_future::time::timeout(Duration::from_millis(100), waiter).await;
        assert!(result.is_ok(), "Waiter should be notified");
        assert!(
            result.unwrap().unwrap(),
            "Waiter should complete successfully"
        );
    }

    #[tokio::test]
    async fn test_multiple_waiters_all_notified() {
        let queue = ProcessQueue::new();
        let cid = test_cid();

        // First caller acquires
        let guard = queue.try_acquire(&cid).await.unwrap();

        // Spawn multiple waiters
        let mut handles = Vec::new();
        for _ in 0..5 {
            let queue_clone = queue.clone();
            handles.push(n0_future::task::spawn(async move {
                let rx = queue_clone.try_acquire(&cid).await.unwrap_err();
                rx.await.unwrap();
            }));
        }

        // Give waiters time to register
        n0_future::time::sleep(Duration::from_millis(10)).await;

        // Explicitly release guard
        guard.release().await;

        // All waiters should complete
        for handle in handles {
            let result = n0_future::time::timeout(Duration::from_millis(100), handle).await;
            assert!(result.is_ok(), "All waiters should be notified");
        }
    }

    #[tokio::test]
    async fn test_reacquire_after_release() {
        let queue = ProcessQueue::new();
        let cid = test_cid();

        // First acquisition
        {
            let guard = queue.try_acquire(&cid).await.unwrap();
            guard.release().await;
        }

        // Should be able to acquire again immediately
        let result = queue.try_acquire(&cid).await;
        assert!(result.is_ok(), "Should be able to reacquire after release");
    }

    #[tokio::test]
    async fn test_drop_releases() {
        let queue = ProcessQueue::new();
        let cid = test_cid();

        // Acquire and drop (not explicit release)
        {
            let _guard = queue.try_acquire(&cid).await.unwrap();
            // Guard dropped here - release is now synchronous
        }

        // Should be able to acquire again immediately (drop is now synchronous)
        let result = queue.try_acquire(&cid).await;
        assert!(result.is_ok(), "Should be able to reacquire after drop");
    }

    #[test]
    fn test_drop_releases_outside_tokio() {
        // Test that dropping ProcessGuard outside tokio runtime works correctly
        // (previously this would permanently lock the CID)
        let queue = ProcessQueue::new();
        let cid = test_cid();

        // Use block_on to acquire in async context, then drop synchronously
        let rt = tokio::runtime::Runtime::new().unwrap();
        let guard = rt.block_on(async { queue.try_acquire(&cid).await.unwrap() });

        // Drop the guard outside async context
        drop(guard);

        // Should be able to acquire again
        let result = rt.block_on(async { queue.try_acquire(&cid).await });
        assert!(
            result.is_ok(),
            "Should be able to reacquire after synchronous drop"
        );
    }

    #[tokio::test]
    async fn test_cancelled_waiters_handled_gracefully() {
        let queue = ProcessQueue::new();
        let cid = test_cid();

        // First caller acquires
        let guard = queue.try_acquire(&cid).await.unwrap();

        // Create waiters then drop them (simulating cancelled tasks)
        {
            let rx1 = queue.try_acquire(&cid).await.unwrap_err();
            let rx2 = queue.try_acquire(&cid).await.unwrap_err();
            // Drop receivers without awaiting - simulates task cancellation
            drop(rx1);
            drop(rx2);
        }

        // Release should handle cancelled receivers gracefully (not panic)
        guard.release().await;

        // Queue should be in clean state
        let result = queue.try_acquire(&cid).await;
        assert!(
            result.is_ok(),
            "Queue should be clean after handling cancelled waiters"
        );
    }

    #[tokio::test]
    async fn test_mixed_cancelled_and_waiting() {
        let queue = ProcessQueue::new();
        let cid = test_cid();

        // First caller acquires
        let guard = queue.try_acquire(&cid).await.unwrap();

        // Create a waiter that will be cancelled
        let rx1 = queue.try_acquire(&cid).await.unwrap_err();
        drop(rx1); // Cancel immediately

        // Create a waiter that will actually wait
        let queue_clone = queue.clone();
        let waiter = n0_future::task::spawn(async move {
            let rx = queue_clone.try_acquire(&cid).await.unwrap_err();
            rx.await.unwrap();
            true
        });

        // Give waiter time to register
        n0_future::time::sleep(Duration::from_millis(10)).await;

        // Release - should notify the waiting task even though one was cancelled
        guard.release().await;

        // Active waiter should still be notified
        let result = n0_future::time::timeout(Duration::from_millis(100), waiter).await;
        assert!(result.is_ok(), "Active waiter should be notified");
        assert!(
            result.unwrap().unwrap(),
            "Active waiter should complete successfully"
        );
    }
}
