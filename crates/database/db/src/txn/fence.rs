//! Commits whose caller stopped waiting, fenced until they land.
//!
//! A commit handed to its transaction's owner (`owner::Owners::finish`)
//! still lands after its caller stops waiting: dropping the caller's future
//! does not undo it. Whatever begins after that drop must see the commit, so
//! the database counts the commits still landing (`CommitFences`) and every
//! new transaction and read waits in `DB::new_txn` while the count is above
//! zero.
//!
//! Each commit handed to an owner carries a [`Fence`]: one state word, which
//! the caller and the owner each try once to move out of `PENDING`. Exactly
//! one wins. A caller that stops waiting raises the count, then swaps to
//! `ABANDONED`; if the owner was done first, it lowers the count back. The
//! owner, finished with the commit however it ended, swaps to `DONE`; if the
//! caller had left, it lowers the count. Whoever lowers the count to zero
//! wakes every waiter, once. A commit whose caller never stops waiting never
//! touches the count, and a begin with nothing fenced costs one load.
//!
//! Orderings. All writes to the count are read-modify-writes, so a load that
//! reads any value at or after a lowering synchronizes with it.
//! * The caller's raise is ordered before the owner's lowering by the swap
//!   pair: the caller's winning swap releases, the owner's failed swap
//!   acquires. The count therefore never dips below the number of fences
//!   still owed, so a begin that happens after the caller's drop reads a
//!   count of at least one until this commit's lowering.
//! * The owner's winning swap releases what the commit published, and the
//!   caller's failed swap acquires it, so a begin after such a drop sees the
//!   commit too.
//! * Each lowering releases and the begin's load acquires: a begin that
//!   reads zero sees every commit whose fence was lowered.
//! * A waiter reads the wake generation before its load, and a lowering to
//!   zero moves the generation after its decrement, so a wake is never lost.
//!
//! The protocol is `proofs/tla/CommitFence.tla`; the swap law is
//! `proofs/lean/CommitFence/Swap.lean`.

use std::sync::atomic::{AtomicU8, AtomicUsize, Ordering};
use std::sync::Arc;

use regolith::sync::Notify;

/// Neither side has moved the state word.
const PENDING: u8 = 0;
/// The caller stopped waiting first: the owner lowers the count.
const ABANDONED: u8 = 1;
/// The owner was finished first: nothing is owed.
const DONE: u8 = 2;

/// A database's count of commits still landing after their caller stopped
/// waiting, and the wake for whoever waits for it to clear.
pub(crate) struct CommitFences {
    outstanding: Count,
    cleared: Notify,
}

/// The count on a cache line of its own: every begin reads it, while the
/// reference count of the `Arc` holding it changes with every explicit
/// commit.
#[repr(align(128))]
struct Count(AtomicUsize);

impl CommitFences {
    pub(crate) fn new() -> Self {
        Self {
            outstanding: Count(AtomicUsize::new(0)),
            cleared: Notify::new(),
        }
    }

    /// Whether a commit whose caller stopped waiting may still be landing.
    #[inline]
    pub(crate) fn any(&self) -> bool {
        self.outstanding.0.load(Ordering::Acquire) != 0
    }

    /// Waits until no commit whose caller stopped waiting is still landing.
    pub(crate) async fn cleared(&self) {
        loop {
            // Armed before the look, so a lowering to zero after it wakes us.
            let cleared = self.cleared.notified();
            if !self.any() {
                return;
            }
            cleared.await;
        }
    }

    fn raise(&self) {
        // The caller's swap that follows publishes this to the owner.
        self.outstanding.0.fetch_add(1, Ordering::Relaxed);
    }

    fn lower(&self) {
        if self.outstanding.0.fetch_sub(1, Ordering::Release) == 1 {
            self.cleared.notify_waiters();
        }
    }
}

impl Default for CommitFences {
    fn default() -> Self {
        Self::new()
    }
}

/// One commit handed to its owner: the state word its caller and owner race
/// on, and the fences it is counted in.
pub(crate) struct Fence {
    state: AtomicU8,
    fences: Arc<CommitFences>,
}

impl Fence {
    pub(crate) fn new(fences: &Arc<CommitFences>) -> Arc<Self> {
        Arc::new(Self {
            state: AtomicU8::new(PENDING),
            fences: Arc::clone(fences),
        })
    }

    /// Whether the caller stopped waiting before the owner began: the owner
    /// abandons the transaction instead of finishing it.
    pub(crate) fn abandoned(&self) -> bool {
        self.state.load(Ordering::Acquire) == ABANDONED
    }

    /// The caller stops waiting. Says whether it left first; if the owner was
    /// already done, the commit is in and nothing stays fenced.
    pub(crate) fn abandon(&self) -> bool {
        self.fences.raise();
        let left_first = self
            .state
            .compare_exchange(PENDING, ABANDONED, Ordering::AcqRel, Ordering::Acquire)
            .is_ok();
        if !left_first {
            self.fences.lower();
        }
        left_first
    }

    /// The owner is finished with the commit: it landed, it failed, or it
    /// was dropped unrun.
    fn finish(&self) {
        let caller_left = self
            .state
            .compare_exchange(PENDING, DONE, Ordering::AcqRel, Ordering::Acquire)
            .is_err();
        if caller_left {
            self.fences.lower();
        }
    }
}

/// The owner's hold on a [`Fence`], carried in the request: dropping it
/// finishes the fence, however the request ends (run to the end, abandoned,
/// dropped unrun, or dropped mid-commit when the worker stops).
pub(crate) struct Landing(Arc<Fence>);

impl Landing {
    pub(crate) fn new(fence: Arc<Fence>) -> Self {
        Self(fence)
    }

    /// Whether the caller stopped waiting before the owner began.
    pub(crate) fn abandoned(&self) -> bool {
        self.0.abandoned()
    }
}

impl Drop for Landing {
    fn drop(&mut self) {
        self.0.finish();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::future::Future;
    use std::pin::pin;
    use std::task::{Context, Poll, Waker};

    fn handed_over(fences: &Arc<CommitFences>) -> (Arc<Fence>, Landing) {
        let fence = Fence::new(fences);
        let landing = Landing::new(Arc::clone(&fence));
        (fence, landing)
    }

    #[test]
    fn a_commit_nobody_cancels_never_touches_the_count() {
        let fences = Arc::new(CommitFences::new());
        let (fence, landing) = handed_over(&fences);
        assert!(!fences.any());
        drop(landing);
        assert!(!fences.any());
        assert!(!fence.abandoned());
        assert_eq!(fences.outstanding.0.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn a_caller_that_leaves_before_the_reply_fences_until_the_owner_finishes() {
        let fences = Arc::new(CommitFences::new());
        let (fence, landing) = handed_over(&fences);
        assert!(
            fence.abandon(),
            "the owner was not done: the caller left first"
        );
        assert!(fences.any(), "the commit may still land");
        assert!(landing.abandoned());
        let mut cx = Context::from_waker(Waker::noop());
        let mut waiting = pin!(fences.cleared());
        assert!(waiting.as_mut().poll(&mut cx).is_pending());
        drop(landing);
        assert!(!fences.any());
        assert_eq!(waiting.as_mut().poll(&mut cx), Poll::Ready(()));
    }

    #[test]
    fn a_caller_that_leaves_after_the_owner_finished_takes_its_raise_back() {
        let fences = Arc::new(CommitFences::new());
        let (fence, landing) = handed_over(&fences);
        drop(landing);
        assert!(!fence.abandon(), "the owner was done first");
        assert!(!fences.any());
        assert_eq!(fences.outstanding.0.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn a_failed_or_dropped_commit_clears_its_fence() {
        let fences = Arc::new(CommitFences::new());
        let (fence, landing) = handed_over(&fences);
        assert!(fence.abandon());
        // The owner never ran the commit (it failed, or its worker stopped):
        // dropping its request is all that clears the fence.
        drop(landing);
        assert!(!fences.any());
        assert_eq!(fences.outstanding.0.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn waiters_wake_once_the_last_fence_clears() {
        let fences = Arc::new(CommitFences::new());
        let (first, first_landing) = handed_over(&fences);
        let (second, second_landing) = handed_over(&fences);
        assert!(first.abandon());
        assert!(second.abandon());
        let mut cx = Context::from_waker(Waker::noop());
        let mut waiting = pin!(fences.cleared());
        assert!(waiting.as_mut().poll(&mut cx).is_pending());
        drop(first_landing);
        assert!(fences.any());
        assert!(waiting.as_mut().poll(&mut cx).is_pending());
        drop(second_landing);
        assert_eq!(waiting.as_mut().poll(&mut cx), Poll::Ready(()));
    }

    #[test]
    fn the_caller_and_the_owner_racing_leave_the_count_exact() {
        for _ in 0..1_000 {
            let fences = Arc::new(CommitFences::new());
            let (fence, landing) = handed_over(&fences);
            let owner = std::thread::spawn(move || drop(landing));
            fence.abandon();
            owner.join().unwrap();
            assert_eq!(fences.outstanding.0.load(Ordering::Relaxed), 0);
        }
    }
}
