//! A channel that carries one value.

use std::cell::RefCell;
use std::future::{poll_fn, Future};
use std::pin::Pin;
use std::sync::Arc;

use super::OneThread;
use std::task::{Context, Poll, Waker};

/// Errors of the channel.
pub mod error {
    use thiserror::Error;

    /// The sender dropped without sending.
    #[derive(Debug, Error, PartialEq, Eq, Clone, Copy)]
    #[error("channel closed")]
    pub struct RecvError(pub(super) ());
}

#[derive(Debug)]
struct State<T> {
    value: Option<T>,
    sender_gone: bool,
    receiver_gone: bool,
    rx_waker: Option<Waker>,
    closed_waker: Option<Waker>,
}

type Shared<T> = Arc<OneThread<RefCell<State<T>>>>;

/// A sender and receiver of one value.
pub fn channel<T>() -> (Sender<T>, Receiver<T>) {
    let shared = Arc::new(OneThread(RefCell::new(State {
        value: None,
        sender_gone: false,
        receiver_gone: false,
        rx_waker: None,
        closed_waker: None,
    })));
    (
        Sender {
            shared: Arc::clone(&shared),
        },
        Receiver { shared },
    )
}

/// The sending half.
pub struct Sender<T> {
    shared: Shared<T>,
}

impl<T> Sender<T> {
    /// Sends `value`, or gives it back when the receiver is gone.
    pub fn send(self, value: T) -> Result<(), T> {
        let mut state = self.shared.borrow_mut();
        if state.receiver_gone {
            return Err(value);
        }
        state.value = Some(value);
        Ok(())
    }

    /// Whether the receiver is gone.
    pub fn is_closed(&self) -> bool {
        self.shared.borrow().receiver_gone
    }

    /// Completes once the receiver is dropped.
    pub async fn closed(&mut self) {
        poll_fn(|cx| {
            let mut state = self.shared.borrow_mut();
            if state.receiver_gone {
                return Poll::Ready(());
            }
            state.closed_waker = Some(cx.waker().clone());
            Poll::Pending
        })
        .await;
    }
}

impl<T> Drop for Sender<T> {
    fn drop(&mut self) {
        let mut state = self.shared.borrow_mut();
        state.sender_gone = true;
        if let Some(waker) = state.rx_waker.take() {
            waker.wake();
        }
    }
}

/// The receiving half; awaiting it gives the value.
pub struct Receiver<T> {
    shared: Shared<T>,
}

impl<T> Future for Receiver<T> {
    type Output = Result<T, error::RecvError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut state = self.shared.borrow_mut();
        if let Some(value) = state.value.take() {
            return Poll::Ready(Ok(value));
        }
        if state.sender_gone {
            return Poll::Ready(Err(error::RecvError(())));
        }
        state.rx_waker = Some(cx.waker().clone());
        Poll::Pending
    }
}

impl<T> Drop for Receiver<T> {
    fn drop(&mut self) {
        let mut state = self.shared.borrow_mut();
        state.receiver_gone = true;
        state.value = None;
        if let Some(waker) = state.closed_waker.take() {
            waker.wake();
        }
    }
}

macro_rules! debug_handle {
    ($($name:ident),+) => {$(
        impl<T> std::fmt::Debug for $name<T> {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.debug_struct(stringify!($name)).finish_non_exhaustive()
            }
        }
    )+};
}

debug_handle!(Sender, Receiver);
