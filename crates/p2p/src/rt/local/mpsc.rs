//! Multi-producer, single-consumer channels: bounded, and unbounded.

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::future::poll_fn;
use std::sync::Arc;
use std::task::{Poll, Waker};

macro_rules! debug_handle {
    ($($name:ident),+) => {$(
        impl<T> std::fmt::Debug for $name<T> {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.debug_struct(stringify!($name)).finish_non_exhaustive()
            }
        }
    )+};
}

use super::semaphore::{Semaphore, TryAcquireError};
use super::OneThread;

/// Errors of the channels.
pub mod error {
    use thiserror::Error;

    /// The receiver is gone; the value comes back.
    #[derive(Debug, Error, PartialEq, Eq, Clone, Copy)]
    #[error("channel closed")]
    pub struct SendError<T>(pub T);

    /// Why [`try_send`](super::Sender::try_send) did not send.
    #[derive(Debug, Error, PartialEq, Eq, Clone, Copy)]
    pub enum TrySendError<T> {
        /// The channel is at capacity.
        #[error("no available capacity")]
        Full(T),
        /// The receiver is gone.
        #[error("channel closed")]
        Closed(T),
    }

    /// Why [`try_recv`](super::Receiver::try_recv) gave nothing.
    #[derive(Debug, Error, PartialEq, Eq, Clone, Copy)]
    pub enum TryRecvError {
        /// Nothing queued, senders remain.
        #[error("receiving on an empty channel")]
        Empty,
        /// Nothing queued and no sender remains.
        #[error("receiving on a closed channel")]
        Disconnected,
    }
}

use error::{SendError, TryRecvError, TrySendError};

struct Chan<T> {
    queue: RefCell<VecDeque<T>>,
    room: Semaphore,
    senders: Cell<usize>,
    rx_closed: Cell<bool>,
    rx_waker: RefCell<Option<Waker>>,
    closed_wakers: RefCell<Vec<Waker>>,
}

impl<T> Chan<T> {
    fn new(capacity: usize) -> Arc<OneThread<Self>> {
        Arc::new(OneThread(Self {
            queue: RefCell::default(),
            room: Semaphore::new(capacity),
            senders: Cell::new(1),
            rx_closed: Cell::new(false),
            rx_waker: RefCell::default(),
            closed_wakers: RefCell::default(),
        }))
    }

    fn wake_receiver(&self) {
        if let Some(waker) = self.rx_waker.borrow_mut().take() {
            waker.wake();
        }
    }

    fn push(&self, value: T) {
        if self.rx_closed.get() {
            return;
        }
        self.queue.borrow_mut().push_back(value);
        self.wake_receiver();
    }

    fn add_sender(&self) {
        self.senders.set(self.senders.get() + 1);
    }

    fn drop_sender(&self) {
        self.senders.set(self.senders.get() - 1);
        if self.senders.get() == 0 {
            self.wake_receiver();
        }
    }

    fn close_receiver(&self) {
        self.rx_closed.set(true);
        self.room.close();
        for waker in self.closed_wakers.borrow_mut().drain(..) {
            waker.wake();
        }
    }

    async fn closed(&self) {
        poll_fn(|cx| {
            if self.rx_closed.get() {
                return Poll::Ready(());
            }
            let mut wakers = self.closed_wakers.borrow_mut();
            if !wakers.iter().any(|waker| waker.will_wake(cx.waker())) {
                wakers.push(cx.waker().clone());
            }
            Poll::Pending
        })
        .await;
    }

    fn try_recv(&self) -> Result<T, TryRecvError> {
        if let Some(value) = self.queue.borrow_mut().pop_front() {
            self.room.add_permits(1);
            return Ok(value);
        }
        if self.senders.get() == 0 || self.rx_closed.get() {
            Err(TryRecvError::Disconnected)
        } else {
            Err(TryRecvError::Empty)
        }
    }

    async fn recv(&self) -> Option<T> {
        poll_fn(|cx| match self.try_recv() {
            Ok(value) => Poll::Ready(Some(value)),
            Err(TryRecvError::Disconnected) => Poll::Ready(None),
            Err(TryRecvError::Empty) => {
                *self.rx_waker.borrow_mut() = Some(cx.waker().clone());
                Poll::Pending
            }
        })
        .await
    }
}

/// A bounded channel of `capacity` messages.
///
/// # Panics
///
/// When `capacity` is zero, as tokio's does.
pub fn channel<T>(capacity: usize) -> (Sender<T>, Receiver<T>) {
    assert!(capacity > 0, "mpsc bounded channel requires buffer > 0");
    let chan = Chan::new(capacity);
    (
        Sender {
            chan: Arc::clone(&chan),
        },
        Receiver { chan },
    )
}

/// A channel without a bound.
pub fn unbounded_channel<T>() -> (UnboundedSender<T>, UnboundedReceiver<T>) {
    let chan = Chan::new(0);
    (
        UnboundedSender {
            chan: Arc::clone(&chan),
        },
        UnboundedReceiver { chan },
    )
}

/// The sending half of a bounded channel.
pub struct Sender<T> {
    chan: Arc<OneThread<Chan<T>>>,
}

/// A sender that does not keep the channel open.
pub struct WeakSender<T> {
    chan: Arc<OneThread<Chan<T>>>,
}

/// A reserved slot of a bounded channel; dropping it frees the slot.
pub struct Permit<'a, T> {
    chan: &'a OneThread<Chan<T>>,
}

impl<T> Sender<T> {
    /// Waits for room and sends `value`; the receiver being gone gives it back.
    pub async fn send(&self, value: T) -> Result<(), SendError<T>> {
        match self.reserve().await {
            Ok(permit) => {
                permit.send(value);
                Ok(())
            }
            Err(_) => Err(SendError(value)),
        }
    }

    /// Sends `value` if there is room now.
    pub fn try_send(&self, value: T) -> Result<(), TrySendError<T>> {
        match self.chan.room.try_acquire() {
            Ok(slot) => slot.forget(),
            Err(TryAcquireError::NoPermits) => return Err(TrySendError::Full(value)),
            Err(TryAcquireError::Closed) => return Err(TrySendError::Closed(value)),
        }
        self.chan.push(value);
        Ok(())
    }

    /// Holds a slot for one message if there is room now.
    pub fn try_reserve(&self) -> Result<Permit<'_, T>, TrySendError<()>> {
        match self.chan.room.try_acquire() {
            Ok(slot) => {
                slot.forget();
                Ok(Permit { chan: &self.chan })
            }
            Err(TryAcquireError::NoPermits) => Err(TrySendError::Full(())),
            Err(TryAcquireError::Closed) => Err(TrySendError::Closed(())),
        }
    }

    /// Waits for room and holds a slot for one message.
    pub async fn reserve(&self) -> Result<Permit<'_, T>, SendError<()>> {
        self.chan
            .room
            .acquire()
            .await
            .map(|slot| {
                slot.forget();
                Permit { chan: &self.chan }
            })
            .map_err(|_| SendError(()))
    }

    /// Slots free right now.
    pub fn capacity(&self) -> usize {
        self.chan.room.available_permits()
    }

    /// Completes once the receiver is gone.
    pub async fn closed(&self) {
        self.chan.closed().await;
    }

    /// Whether the receiver is gone.
    pub fn is_closed(&self) -> bool {
        self.chan.rx_closed.get()
    }

    /// A sender that does not keep the channel open.
    pub fn downgrade(&self) -> WeakSender<T> {
        WeakSender {
            chan: Arc::clone(&self.chan),
        }
    }
}

impl<T> Permit<'_, T> {
    /// Sends into the reserved slot.
    pub fn send(self, value: T) {
        self.chan.push(value);
        std::mem::forget(self);
    }
}

impl<T> Drop for Permit<'_, T> {
    fn drop(&mut self) {
        self.chan.room.add_permits(1);
    }
}

impl<T> WeakSender<T> {
    /// A sender, unless every one is gone.
    pub fn upgrade(&self) -> Option<Sender<T>> {
        (self.chan.senders.get() > 0).then(|| {
            self.chan.add_sender();
            Sender {
                chan: Arc::clone(&self.chan),
            }
        })
    }
}

impl<T> Clone for WeakSender<T> {
    fn clone(&self) -> Self {
        Self {
            chan: Arc::clone(&self.chan),
        }
    }
}

impl<T> Clone for Sender<T> {
    fn clone(&self) -> Self {
        self.chan.add_sender();
        Self {
            chan: Arc::clone(&self.chan),
        }
    }
}

impl<T> Drop for Sender<T> {
    fn drop(&mut self) {
        self.chan.drop_sender();
    }
}

/// The receiving half of a bounded channel.
pub struct Receiver<T> {
    chan: Arc<OneThread<Chan<T>>>,
}

impl<T> Receiver<T> {
    /// The next message, `None` once every sender is gone and the queue empty.
    pub async fn recv(&mut self) -> Option<T> {
        self.chan.recv().await
    }

    /// The next message if one is queued.
    pub fn try_recv(&mut self) -> Result<T, TryRecvError> {
        self.chan.try_recv()
    }

    /// Refuses new messages; queued ones can still be received.
    pub fn close(&mut self) {
        self.chan.close_receiver();
    }

    /// Whether the channel is closed: by [`close`](Self::close), or because
    /// every sender is gone.
    pub fn is_closed(&self) -> bool {
        self.chan.rx_closed.get() || self.chan.senders.get() == 0
    }
}

impl<T> Drop for Receiver<T> {
    fn drop(&mut self) {
        self.chan.close_receiver();
        self.chan.queue.borrow_mut().clear();
    }
}

/// The sending half of an unbounded channel.
pub struct UnboundedSender<T> {
    chan: Arc<OneThread<Chan<T>>>,
}

impl<T> UnboundedSender<T> {
    /// Sends `value`; the receiver being gone gives it back.
    pub fn send(&self, value: T) -> Result<(), SendError<T>> {
        if self.chan.rx_closed.get() {
            return Err(SendError(value));
        }
        self.chan.push(value);
        Ok(())
    }

    /// Completes once the receiver is gone.
    pub async fn closed(&self) {
        self.chan.closed().await;
    }

    /// Whether the receiver is gone.
    pub fn is_closed(&self) -> bool {
        self.chan.rx_closed.get()
    }
}

impl<T> Clone for UnboundedSender<T> {
    fn clone(&self) -> Self {
        self.chan.add_sender();
        Self {
            chan: Arc::clone(&self.chan),
        }
    }
}

impl<T> Drop for UnboundedSender<T> {
    fn drop(&mut self) {
        self.chan.drop_sender();
    }
}

/// The receiving half of an unbounded channel.
pub struct UnboundedReceiver<T> {
    chan: Arc<OneThread<Chan<T>>>,
}

impl<T> UnboundedReceiver<T> {
    /// The next message, `None` once every sender is gone and the queue empty.
    pub async fn recv(&mut self) -> Option<T> {
        self.chan.recv().await
    }

    /// The next message if one is queued.
    pub fn try_recv(&mut self) -> Result<T, TryRecvError> {
        self.chan.try_recv()
    }

    /// Refuses new messages; queued ones can still be received.
    pub fn close(&mut self) {
        self.chan.close_receiver();
    }
}

impl<T> Drop for UnboundedReceiver<T> {
    fn drop(&mut self) {
        self.chan.close_receiver();
        self.chan.queue.borrow_mut().clear();
    }
}

debug_handle!(
    Sender,
    WeakSender,
    Receiver,
    UnboundedSender,
    UnboundedReceiver
);
