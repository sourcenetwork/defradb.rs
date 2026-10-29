//! The runtime on wasm32: the browser's event loop, through
//! wasm-bindgen-futures and browser timers. Nothing is built and nothing is
//! dropped; shutdown stops accepting and drains, and the page's loop goes
//! on. A panic aborts the module, so containment cannot catch it here.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;

pub(crate) use n0_future::time::{Instant, Sleep};

use crate::config::EdgeConfig;
use crate::error::EdgeError;
use crate::gate::Gate;
use crate::handle::{drain_gate, Drained};
use crate::shared::Shared;

/// Where network work runs: the browser's event loop.
#[derive(Clone)]
pub(crate) struct Rt;

impl Rt {
    pub(crate) fn spawn<F>(&self, future: F) -> JoinHandle<F::Output>
    where
        F: Future + 'static,
        F::Output: 'static,
    {
        JoinHandle(n0_future::task::spawn(future))
    }

    /// One thread runs everything, so blocking work runs as a task.
    pub(crate) fn blocking<F, T>(&self, work: F) -> JoinHandle<T>
    where
        F: FnOnce() -> T + 'static,
        T: 'static,
    {
        JoinHandle(n0_future::task::spawn(async move { work() }))
    }

    pub(crate) fn sleep_until(&self, at: Instant) -> Sleep {
        n0_future::time::sleep_until(at)
    }

    pub(crate) fn poll_sleep(&self, sleep: Pin<&mut Sleep>, cx: &mut Context<'_>) -> Poll<()> {
        sleep.poll(cx)
    }
}

/// A spawned task's completion. Dropping it leaves the task running.
pub(crate) struct JoinHandle<T>(n0_future::task::JoinHandle<T>);

impl<T> JoinHandle<T> {
    pub(crate) fn abort(&self) {
        self.0.abort();
    }

    pub(crate) fn is_finished(&self) -> bool {
        self.0.is_finished()
    }
}

impl<T> Future for JoinHandle<T> {
    type Output = Result<T, EdgeError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        Pin::new(&mut self.0).poll(cx).map(|joined| {
            joined.map_err(|error| {
                if error.is_panic() {
                    EdgeError::Panicked(error.to_string())
                } else {
                    EdgeError::Cancelled
                }
            })
        })
    }
}

/// The event loop belongs to the page; the owner keeps only how long a
/// shutdown drains.
pub(crate) struct Owner {
    drain: Duration,
    gate: Arc<Gate>,
    shared: Arc<Shared>,
}

impl Owner {
    pub(crate) fn start(
        config: &EdgeConfig,
        gate: &Arc<Gate>,
    ) -> Result<(Arc<Shared>, Self), EdgeError> {
        let shared = Shared::new(Rt, config.deadline);
        Ok((
            Arc::clone(&shared),
            Self {
                drain: config.drain,
                gate: Arc::clone(gate),
                shared,
            },
        ))
    }

    pub(crate) fn request_stop(&self) {
        self.gate.close();
    }

    pub(crate) async fn finish(&mut self) -> Result<Drained, EdgeError> {
        self.request_stop();
        Ok(drain_gate(&self.gate, &self.shared, self.drain).await)
    }
}
