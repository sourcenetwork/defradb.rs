//! A channel holding one value that receivers wait to see change.

use std::cell::{Cell, Ref, RefCell};
use std::future::poll_fn;
use std::sync::Arc;

use super::OneThread;
use std::task::{Poll, Waker};

/// Errors of the channel.
pub mod error {
    use thiserror::Error;

    /// The sender is gone.
    #[derive(Debug, Error, PartialEq, Eq, Clone, Copy)]
    #[error("channel closed")]
    pub struct RecvError(pub(super) ());
}

struct Shared<T> {
    value: RefCell<T>,
    sender_gone: Cell<bool>,
    wakers: RefCell<Vec<Waker>>,
}

/// A channel starting at `init`.
pub fn channel<T>(init: T) -> (Sender<T>, Receiver<T>) {
    let shared = Arc::new(OneThread(Shared {
        value: RefCell::new(init),
        sender_gone: Cell::new(false),
        wakers: RefCell::default(),
    }));
    (
        Sender {
            shared: Arc::clone(&shared),
        },
        Receiver { shared },
    )
}

/// The sending half.
pub struct Sender<T> {
    shared: Arc<OneThread<Shared<T>>>,
}

impl<T> Sender<T> {
    /// Replaces the value, wakes the receivers and gives back the old one.
    pub fn send_replace(&self, value: T) -> T {
        let old = self.shared.value.replace(value);
        for waker in self.shared.wakers.borrow_mut().drain(..) {
            waker.wake();
        }
        old
    }
}

impl<T> Drop for Sender<T> {
    fn drop(&mut self) {
        self.shared.sender_gone.set(true);
        for waker in self.shared.wakers.borrow_mut().drain(..) {
            waker.wake();
        }
    }
}

/// A receiving half; clones see the same value.
pub struct Receiver<T> {
    shared: Arc<OneThread<Shared<T>>>,
}

impl<T> Receiver<T> {
    /// Waits until `condition` holds for the value, and borrows it. Fails if
    /// the sender is gone while it does not.
    pub async fn wait_for<F>(&mut self, mut condition: F) -> Result<Ref<'_, T>, error::RecvError>
    where
        F: FnMut(&T) -> bool,
    {
        poll_fn(|cx| {
            if condition(&self.shared.value.borrow()) {
                return Poll::Ready(true);
            }
            if self.shared.sender_gone.get() {
                return Poll::Ready(false);
            }
            let mut wakers = self.shared.wakers.borrow_mut();
            if !wakers.iter().any(|waker| waker.will_wake(cx.waker())) {
                wakers.push(cx.waker().clone());
            }
            Poll::Pending
        })
        .await
        .then(|| self.shared.value.borrow())
        .ok_or(error::RecvError(()))
    }
}

impl<T> Clone for Receiver<T> {
    fn clone(&self) -> Self {
        Self {
            shared: Arc::clone(&self.shared),
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
