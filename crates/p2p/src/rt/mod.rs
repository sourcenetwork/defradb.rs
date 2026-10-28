//! The runtime names this crate's code uses, one set per target: nuclei
//! natively, the browser's event loop (n0-future, and the single-threaded
//! [`local`] channel, semaphore and select primitives) on wasm32. Code written
//! against this module builds and behaves the same on both.
//!
//! A nuclei task is cancelled when its handle drops, so a task that must
//! outlive its handle is [`Task::detach`]ed; the browser's tasks run on either
//! way.

pub mod local;

#[cfg(not(target_arch = "wasm32"))]
mod native {
    pub use nuclei::sync::{mpsc, oneshot, watch, OwnedSemaphorePermit, Semaphore};
    pub use nuclei::task::{yield_now, AbortHandle, JoinError, JoinSet, Task};
    pub use nuclei::{select, spawn};
    pub use regolith::sync::Notify;

    /// `future` inside the tokio context iroh's sockets and timers need. A task that drives iroh is
    /// wrapped once, at its outermost future.
    pub fn compat<F>(future: F) -> edge::Compat<F> {
        edge::compat(future)
    }

    /// The next finished task of `set`, `None` once it is empty.
    pub async fn join_next<T>(set: &mut JoinSet<T>) -> Option<Result<T, JoinError>> {
        set.join_next().await
    }

    /// Clocks, sleeps and deadlines.
    pub mod time {
        pub use nuclei::time::{
            interval, sleep, sleep_until, timeout, Instant, Interval, MissedTickBehavior, Sleep,
        };

        /// The clock iroh-gossip's protocol state machine takes: the type of
        /// that library (n0-future's, tokio's `Instant` natively), not of the
        /// runtime. Only the type is named; nothing runs on tokio.
        pub use n0_future::time::Instant as GossipInstant;
    }
}

#[cfg(target_arch = "wasm32")]
mod wasm {
    use std::future::Future;

    pub use super::local::{mpsc, oneshot, watch, OwnedSemaphorePermit, Semaphore};
    pub use crate::local_select as select;
    pub use n0_future::task::{AbortHandle, JoinError, JoinSet};
    pub use regolith::sync::Notify;

    /// Clocks, sleeps and deadlines.
    pub mod time {
        pub use n0_future::time::{
            interval, sleep, sleep_until, timeout, Instant, Interval, MissedTickBehavior, Sleep,
        };

        /// The clock iroh-gossip's protocol state machine takes.
        pub use n0_future::time::Instant as GossipInstant;
    }

    /// A spawned task. Unlike nuclei's, dropping it leaves the task running
    /// (the browser's loop owns it), so [`detach`](Self::detach) only ends the
    /// handle.
    #[derive(Debug)]
    pub struct Task<T>(n0_future::task::JoinHandle<T>);

    impl<T> Task<T> {
        /// Lets the task run on without its handle.
        pub fn detach(self) {}

        /// Cancels the task.
        pub fn abort(&self) {
            self.0.abort();
        }

        /// Whether the task has ended.
        pub fn is_finished(&self) -> bool {
            self.0.is_finished()
        }

        /// A handle that cancels the task from anywhere.
        pub fn abort_handle(&self) -> AbortHandle {
            self.0.abort_handle()
        }

        /// The task's output, or why there is none.
        pub fn join(self) -> impl Future<Output = Result<T, JoinError>> {
            self.0
        }
    }

    /// The next finished task of `set`, `None` once it is empty.
    pub async fn join_next<T>(set: &mut JoinSet<T>) -> Option<Result<T, JoinError>> {
        std::future::poll_fn(|cx| set.poll_join_next(cx)).await
    }

    /// The browser needs no runtime context: `future` as it is.
    pub fn compat<F>(future: F) -> F {
        future
    }

    /// Gives the other tasks a turn: pending once, waking itself.
    pub async fn yield_now() {
        let mut yielded = false;
        std::future::poll_fn(|cx| {
            if yielded {
                return std::task::Poll::Ready(());
            }
            yielded = true;
            cx.waker().wake_by_ref();
            std::task::Poll::Pending
        })
        .await;
    }

    /// Runs `future` on the browser's event loop.
    pub fn spawn<F>(future: F) -> Task<F::Output>
    where
        F: Future + 'static,
        F::Output: 'static,
    {
        Task(n0_future::task::spawn(future))
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub use native::*;
#[cfg(target_arch = "wasm32")]
pub use wasm::*;
