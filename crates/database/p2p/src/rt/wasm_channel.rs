//! nuclei's `sync::mpsc` and `sync::oneshot` calls over `futures::channel`, for
//! the browser build. futures' bounded mpsc holds capacity plus one slot per
//! sender and per send waiting, on the single-threaded browser build only.

use std::cell::RefCell;
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use futures::channel::{mpsc as fmpsc, oneshot as foneshot};
use futures::future::FusedFuture;
use futures::task::noop_waker_ref;
use futures::StreamExt;

pub mod mpsc {
    //! A bounded channel with `&self` sends and an `Option`-returning receive.

    use super::*;

    /// The error types of the channel.
    pub mod error {
        use std::fmt;

        /// The receiver is gone; the value comes back.
        #[derive(PartialEq, Eq, Clone, Copy)]
        pub struct SendError<T>(pub T);

        impl<T> fmt::Debug for SendError<T> {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.debug_struct("SendError").finish_non_exhaustive()
            }
        }

        impl<T> fmt::Display for SendError<T> {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("channel closed")
            }
        }

        impl<T> std::error::Error for SendError<T> {}

        /// A send that never waits could not complete.
        #[derive(PartialEq, Eq, Clone, Copy)]
        pub enum TrySendError<T> {
            /// The channel is full.
            Full(T),
            /// The receiver is gone.
            Closed(T),
        }

        impl<T> TrySendError<T> {
            /// The value that was not sent.
            pub fn into_inner(self) -> T {
                match self {
                    Self::Full(value) | Self::Closed(value) => value,
                }
            }
        }

        impl<T> fmt::Debug for TrySendError<T> {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                match self {
                    Self::Full(_) => f.write_str("Full(..)"),
                    Self::Closed(_) => f.write_str("Closed(..)"),
                }
            }
        }

        impl<T> fmt::Display for TrySendError<T> {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                match self {
                    Self::Full(_) => f.write_str("no available capacity"),
                    Self::Closed(_) => f.write_str("channel closed"),
                }
            }
        }

        impl<T> std::error::Error for TrySendError<T> {}

        /// A receive that never waits found nothing.
        #[derive(PartialEq, Eq, Clone, Copy, Debug)]
        pub enum TryRecvError {
            /// Nothing queued, senders remain.
            Empty,
            /// Nothing queued, no sender remains.
            Disconnected,
        }

        impl fmt::Display for TryRecvError {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                match self {
                    Self::Empty => f.write_str("receiving on an empty channel"),
                    Self::Disconnected => f.write_str("receiving on a closed channel"),
                }
            }
        }

        impl std::error::Error for TryRecvError {}
    }

    use error::{SendError, TryRecvError, TrySendError};

    /// A bounded channel of `buffer` messages.
    pub fn channel<T>(buffer: usize) -> (Sender<T>, Receiver<T>) {
        let (tx, rx) = fmpsc::channel(buffer);
        (Sender(RefCell::new(tx)), Receiver(rx))
    }

    /// The sending half; clones feed the same receiver.
    pub struct Sender<T>(RefCell<fmpsc::Sender<T>>);

    impl<T> Sender<T> {
        /// Waits for room, then queues `value`. Dropping the future before it
        /// completes drops the value.
        pub fn send(&self, value: T) -> impl Future<Output = Result<(), SendError<T>>> + '_ {
            // futures keeps one waker per sender: two tasks waiting on a shared one would
            // overwrite each other's, so each send waits on a sender of its own.
            let mut tx = self.0.borrow().clone();
            let mut value = Some(value);
            std::future::poll_fn(move |cx| match tx.poll_ready(cx) {
                Poll::Pending => Poll::Pending,
                Poll::Ready(ready) => {
                    let Some(value) = value.take() else {
                        return Poll::Pending;
                    };
                    Poll::Ready(match ready {
                        Ok(()) => tx
                            .try_send(value)
                            .map_err(|error| SendError(error.into_inner())),
                        Err(_) => Err(SendError(value)),
                    })
                }
            })
        }

        /// Queues `value` if there is room now.
        pub fn try_send(&self, value: T) -> Result<(), TrySendError<T>> {
            self.0.borrow_mut().try_send(value).map_err(|error| {
                let full = error.is_full();
                let value = error.into_inner();
                if full {
                    TrySendError::Full(value)
                } else {
                    TrySendError::Closed(value)
                }
            })
        }

        /// Takes a slot now if there is room, to send into later.
        pub fn try_reserve(&self) -> Result<Permit<'_, T>, TrySendError<()>> {
            let mut cx = Context::from_waker(noop_waker_ref());
            match self.0.borrow_mut().poll_ready(&mut cx) {
                Poll::Ready(Ok(())) => Ok(Permit(self)),
                Poll::Ready(Err(_)) => Err(TrySendError::Closed(())),
                Poll::Pending => Err(TrySendError::Full(())),
            }
        }

        /// Whether the receiver is gone.
        pub fn is_closed(&self) -> bool {
            self.0.borrow().is_closed()
        }
    }

    // SAFETY: without wasm atomics the build has one thread, and every borrow
    // of the cell is short and never held across an await. Enabling atomics
    // removes this impl's cfg and fails the build where a `Sync` sender is
    // needed.
    #[cfg(not(target_feature = "atomics"))]
    unsafe impl<T: Send> Sync for Sender<T> {}

    impl<T> Clone for Sender<T> {
        fn clone(&self) -> Self {
            Self(RefCell::new(self.0.borrow().clone()))
        }
    }

    impl<T> fmt::Debug for Sender<T> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.debug_struct("Sender").finish_non_exhaustive()
        }
    }

    /// A slot taken by [`Sender::try_reserve`]; dropping it without sending
    /// gives nothing up.
    #[derive(Debug)]
    pub struct Permit<'a, T>(&'a Sender<T>);

    impl<T> Permit<'_, T> {
        /// Queues `value` in the reserved slot.
        pub fn send(self, value: T) {
            let _ = self.0 .0.borrow_mut().start_send(value);
        }
    }

    /// The receiving half.
    pub struct Receiver<T>(fmpsc::Receiver<T>);

    impl<T> Receiver<T> {
        /// The next message, `None` once every sender is gone and the queue is empty.
        pub fn recv(&mut self) -> impl Future<Output = Option<T>> + '_ {
            self.0.next()
        }

        /// The next message if one is queued now.
        pub fn try_recv(&mut self) -> Result<T, TryRecvError> {
            self.0.try_recv().map_err(|error| match error {
                fmpsc::TryRecvError::Empty => TryRecvError::Empty,
                fmpsc::TryRecvError::Closed => TryRecvError::Disconnected,
            })
        }

        /// Refuses new sends; queued messages can still be received.
        pub fn close(&mut self) {
            self.0.close();
        }
    }

    impl<T> fmt::Debug for Receiver<T> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.debug_struct("Receiver").finish_non_exhaustive()
        }
    }
}

pub mod oneshot {
    //! A channel that carries one value.

    use super::*;

    /// The error types of the channel.
    pub mod error {
        use std::fmt;

        /// The sender was dropped without sending.
        #[derive(Debug, Eq, PartialEq, Clone)]
        pub struct RecvError(pub(super) ());

        impl fmt::Display for RecvError {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("channel closed")
            }
        }

        impl std::error::Error for RecvError {}

        /// A receive that never waits found nothing.
        #[derive(Debug, Eq, PartialEq, Clone)]
        pub enum TryRecvError {
            /// Nothing sent yet.
            Empty,
            /// The sender is gone, or the value was already received.
            Closed,
        }

        impl fmt::Display for TryRecvError {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                match self {
                    Self::Empty => f.write_str("channel empty"),
                    Self::Closed => f.write_str("channel closed"),
                }
            }
        }

        impl std::error::Error for TryRecvError {}
    }

    use error::{RecvError, TryRecvError};

    /// A channel for one value.
    pub fn channel<T>() -> (Sender<T>, Receiver<T>) {
        let (tx, rx) = foneshot::channel();
        (Sender(tx), Receiver(rx))
    }

    /// The sending half.
    #[derive(Debug)]
    pub struct Sender<T>(foneshot::Sender<T>);

    impl<T> Sender<T> {
        /// Sends `value`, handing it back if the receiver is gone.
        pub fn send(self, value: T) -> Result<(), T> {
            self.0.send(value)
        }

        /// Whether the receiver is gone.
        pub fn is_closed(&self) -> bool {
            self.0.is_canceled()
        }

        /// Resolves once the receiver is gone.
        pub async fn closed(&mut self) {
            std::future::poll_fn(|cx| self.0.poll_canceled(cx)).await;
        }
    }

    /// The receiving half; awaiting it yields the value.
    #[derive(Debug)]
    pub struct Receiver<T>(foneshot::Receiver<T>);

    impl<T> Receiver<T> {
        /// The value if it was sent already.
        pub fn try_recv(&mut self) -> Result<T, TryRecvError> {
            match self.0.try_recv() {
                Ok(Some(value)) => Ok(value),
                Ok(None) => Err(TryRecvError::Empty),
                Err(foneshot::Canceled) => Err(TryRecvError::Closed),
            }
        }

        /// Refuses a later send.
        pub fn close(&mut self) {
            self.0.close();
        }
    }

    impl<T> Future for Receiver<T> {
        type Output = Result<T, RecvError>;

        fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
            Pin::new(&mut self.0)
                .poll(cx)
                .map_err(|foneshot::Canceled| RecvError(()))
        }
    }

    impl<T> FusedFuture for Receiver<T> {
        fn is_terminated(&self) -> bool {
            self.0.is_terminated()
        }
    }
}
