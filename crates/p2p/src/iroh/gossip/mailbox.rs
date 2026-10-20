//! A bounded queue its one reader waits on through a [`Notify`].
//!
//! kovan's `recv_async` leaves a waiter behind each time it is polled and
//! not ready, which a loop that also waits on timers does constantly. A
//! `Notified` future deregisters when dropped, so waiting through one is safe
//! in a `select!`.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use regolith::sync::{bounded, BoundedReceiver, BoundedSender, Notify};

/// Wakes a reader that may wait on several mailboxes at once.
pub(super) type Wake = Arc<Notify>;

pub(super) struct Mailbox<T>
where
    T: 'static,
{
    tx: BoundedSender<T>,
    rx: BoundedReceiver<T>,
    wake: Wake,
    closed: AtomicBool,
}

impl<T> Mailbox<T>
where
    T: 'static,
{
    pub(super) fn new(capacity: usize, wake: Wake) -> Self {
        let (tx, rx) = bounded(capacity);
        Self {
            tx,
            rx,
            wake,
            closed: AtomicBool::new(false),
        }
    }

    /// Waits for room, then queues `item`.
    pub(super) async fn push(&self, item: T) {
        self.tx.send_async(item).await;
        self.wake.notify_one();
    }

    /// Queues `item` if there is room, or hands it back.
    pub(super) fn try_push(&self, item: T) -> Result<(), T> {
        self.tx.try_send(item)?;
        self.wake.notify_one();
        Ok(())
    }

    pub(super) fn pop(&self) -> Option<T> {
        self.rx.try_recv()
    }

    /// Tells the reader no more items will come.
    pub(super) fn close(&self) {
        self.closed.store(true, Ordering::Release);
        self.wake.notify_one();
    }

    pub(super) fn is_closed(&self) -> bool {
        self.closed.load(Ordering::Acquire)
    }

    /// The next item, waiting for one; `None` once closed and empty.
    pub(super) async fn next(&self) -> Option<T> {
        loop {
            // Read the flag before looking, so an item queued just before
            // the close is still returned.
            let closed = self.is_closed();
            if let Some(item) = self.pop() {
                return Some(item);
            }
            if closed {
                return None;
            }
            self.wake.notified().await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn items_queued_before_close_are_still_read() {
        let mailbox = Mailbox::new(2, Wake::default());
        mailbox.push(1).await;
        mailbox.close();
        assert_eq!(mailbox.next().await, Some(1));
        assert_eq!(mailbox.next().await, None);
    }

    #[tokio::test]
    async fn a_full_mailbox_hands_the_item_back() {
        let mailbox = Mailbox::new(1, Wake::default());
        mailbox.try_push(1).unwrap();
        assert_eq!(mailbox.try_push(2), Err(2));
    }
}
