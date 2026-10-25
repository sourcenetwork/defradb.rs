//! The transactions a store has handed out and not yet seen end.
//!
//! `close` waits for them on a [`Latch`] the last one counts down, with a
//! deadline, rather than polling a count. Every transaction holds a
//! [`TxnSlot`] for as long as it lives, its commit and post-commit work
//! included, and gives it back by dropping it, so a slot is returned exactly
//! once on every path.
//!
//! The count and the closing flag are read in opposite orders by `close`
//! and by the transaction that ends last (each writes one, then reads the
//! other, all sequentially consistent), so at least one of them sees the
//! other: either `close` finds nothing open, or the last transaction finds
//! the store closing and opens the latch.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use regolith::sync::Latch;

use crate::corekv::{Error, Result};

pub(super) struct TxnSlots {
    open: AtomicUsize,
    closing: AtomicBool,
    idle: Latch,
}

/// One transaction's place in its store's count, given back on drop.
pub(super) struct TxnSlot(Arc<TxnSlots>);

impl Drop for TxnSlot {
    fn drop(&mut self) {
        if self.0.open.fetch_sub(1, Ordering::SeqCst) == 1 && self.0.closing.load(Ordering::SeqCst)
        {
            self.0.idle.count_down();
        }
    }
}

impl TxnSlots {
    pub(super) fn new() -> Arc<Self> {
        Arc::new(Self {
            open: AtomicUsize::new(0),
            closing: AtomicBool::new(false),
            idle: Latch::new(1),
        })
    }

    /// A slot for a new transaction, refused once the store is closing.
    pub(super) fn enter(self: &Arc<Self>) -> Result<TxnSlot> {
        if self.is_closing() {
            return Err(Error::DBClosed);
        }
        self.open.fetch_add(1, Ordering::SeqCst);
        let slot = TxnSlot(Arc::clone(self));
        // Closing between the check and the count: the slot handed back
        // here is what `close` may be waiting for.
        if self.is_closing() {
            return Err(Error::DBClosed);
        }
        Ok(slot)
    }

    pub(super) fn is_closing(&self) -> bool {
        self.closing.load(Ordering::SeqCst)
    }

    /// Marks the store closing; `false` when it already was.
    pub(super) fn begin_close(&self) -> bool {
        !self.closing.swap(true, Ordering::SeqCst)
    }

    /// Waits until every slot is given back, or `timeout` passes.
    pub(super) async fn quiesce(&self, timeout: Duration) -> Result<()> {
        if self.open.load(Ordering::SeqCst) == 0 {
            return Ok(());
        }
        // The deadline is a timer on the pool's deadline queue: the calling
        // worker's, or the process's default pool's off the pool.
        match workers::timeout(timeout, self.idle.wait()).await {
            Ok(()) => Ok(()),
            Err(workers::Error::TimedOut) => Err(Error::Backend(format!(
                "closed with {} transaction(s) still in flight",
                self.open.load(Ordering::SeqCst)
            ))),
            Err(error) => Err(Error::Backend(format!(
                "close could not wait for its transactions: {error}"
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_slot_taken_while_closing_is_refused_and_given_back() {
        let slots = TxnSlots::new();
        let held = slots.enter().unwrap();
        assert!(slots.begin_close());
        assert!(!slots.begin_close());
        assert!(matches!(slots.enter(), Err(Error::DBClosed)));
        assert_eq!(slots.open.load(Ordering::SeqCst), 1);
        assert!(!slots.idle.is_set());
        drop(held);
        assert!(slots.idle.is_set());
    }

    #[test]
    fn slots_given_back_before_close_leave_the_latch_shut() {
        let slots = TxnSlots::new();
        drop(slots.enter().unwrap());
        assert!(!slots.idle.is_set());
        assert!(slots.begin_close());
        assert_eq!(slots.open.load(Ordering::SeqCst), 0);
    }
}

#[cfg(test)]
mod close_tests {
    use super::*;
    use crate::backends::{RegolithStore, RegolithStoreOptions};
    use crate::corekv::Store;
    use std::future::Future;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::task::{Context, Poll, Wake, Waker};

    #[derive(Default)]
    struct CountingWaker(AtomicUsize);
    impl Wake for CountingWaker {
        fn wake(self: Arc<Self>) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    /// The transaction that ends last wakes `close`; nothing polls a clock.
    #[test]
    fn close_is_woken_by_the_last_transaction_that_ends() {
        workers::block_on(async {
            let store = RegolithStore::in_memory().unwrap();
            let reader = store.new_txn(true).await.unwrap();
            let mut writer = store.new_txn(false).await.unwrap();
            writer.set(b"key", b"value").await.unwrap();
            let counter = Arc::new(CountingWaker::default());
            let waker = Waker::from(Arc::clone(&counter));
            let mut cx = Context::from_waker(&waker);
            let mut close = Box::pin(store.close());
            assert!(close.as_mut().poll(&mut cx).is_pending());
            assert!(matches!(store.new_txn(false).await, Err(Error::DBClosed)));

            drop(reader);
            assert_eq!(counter.0.load(Ordering::SeqCst), 0);
            writer.commit().await.unwrap();
            assert_eq!(counter.0.load(Ordering::SeqCst), 1);
            assert!(matches!(close.as_mut().poll(&mut cx), Poll::Ready(Ok(()))));
        })
        .unwrap()
    }

    #[test]
    fn close_reports_a_transaction_still_open_at_its_deadline() {
        workers::block_on(async {
            let mut options = RegolithStoreOptions::memory();
            options.close_timeout = std::time::Duration::from_millis(20);
            let store = RegolithStore::open_with_options("close-deadline", options).unwrap();
            let held = store.new_txn(false).await.unwrap();
            let error = store.close().await.unwrap_err();
            assert!(error
                .to_string()
                .contains("1 transaction(s) still in flight"));
            drop(held);
        })
        .unwrap()
    }
}
