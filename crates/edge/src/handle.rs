//! The narrow handle every network component receives at construction.

use std::fmt;
use std::future::{poll_fn, Future};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::pin::pin;
use std::sync::Arc;
use std::task::Poll;
use std::time::Duration;

use defra_core::thread_bounds::MaybeSend;
use futures_lite::FutureExt;

use crate::contain::contain;
use crate::error::EdgeError;
use crate::gate::{Gate, Permit};
use crate::platform::Instant;
use crate::shared::Shared;
use crate::stats::EdgeStats;
use crate::task::{Service, Task};
use crate::timer::Timer;

/// A handle onto the network runtime: spawn network work, wait on network
/// deadlines, and, natively, block a plain thread on async work.
///
/// Cloning is cheap. Every network component takes one at construction;
/// nothing finds a runtime on its own. Work is admitted until the runtime
/// (or the [`Scope`] this handle came from) stops accepting; after that a
/// spawn's completion is [`EdgeError::Closed`].
///
/// ```no_run
/// # async fn example() -> Result<(), edge::EdgeError> {
/// let runtime = edge::EdgeRuntime::start(edge::EdgeConfig::default())?;
/// let edge = runtime.edge().clone();
/// let answer = edge.spawn(async { 6 * 7 }).await?;
/// assert_eq!(answer, 42);
/// # Ok(())
/// # }
/// ```
#[derive(Clone)]
pub struct Edge {
    pub(crate) shared: Arc<Shared>,
    pub(crate) gate: Arc<Gate>,
}

impl Edge {
    pub(crate) fn new(shared: Arc<Shared>, gate: Arc<Gate>) -> Self {
        Self { shared, gate }
    }

    fn admit(&self) -> Option<Permit> {
        let permit = self.gate.admit();
        if permit.is_none() {
            self.shared.stats.refused();
            tracing::debug!("network work refused: the network runtime is shutting down");
        }
        permit
    }

    /// Runs `future` as a network task. A panic in it fails only its own
    /// completion.
    pub fn spawn<F>(&self, future: F) -> Task<F::Output>
    where
        F: Future + MaybeSend + 'static,
        F::Output: MaybeSend + 'static,
    {
        self.try_spawn(future).unwrap_or_else(|_| Task::refused())
    }

    /// [`spawn`](Self::spawn), or `future` back, unstarted, when the
    /// runtime or scope no longer accepts work, for a caller that must run
    /// it all the same.
    pub fn try_spawn<F>(&self, future: F) -> Result<Task<F::Output>, F>
    where
        F: Future + MaybeSend + 'static,
        F::Output: MaybeSend + 'static,
    {
        let Some(permit) = self.admit() else {
            return Err(future);
        };
        let shared = Arc::clone(&self.shared);
        let join = self.shared.rt.spawn(async move {
            let _permit = permit;
            contain(&shared.stats, AssertUnwindSafe(future).catch_unwind().await)
        });
        Ok(Task::running(&self.shared, join))
    }

    /// Runs `work`, which blocks, on a thread set aside for blocking work.
    pub fn spawn_blocking<F, T>(&self, work: F) -> Task<T>
    where
        F: FnOnce() -> T + MaybeSend + 'static,
        T: MaybeSend + 'static,
    {
        let Some(permit) = self.admit() else {
            return Task::refused();
        };
        let shared = Arc::clone(&self.shared);
        let join = self.shared.rt.blocking(move || {
            let _permit = permit;
            contain(&shared.stats, catch_unwind(AssertUnwindSafe(work)))
        });
        Task::running(&self.shared, join)
    }

    /// Runs the future `make` builds on one thread from start to end:
    /// `make` runs on that thread, and the future never moves, so it need
    /// not be `Send` and whatever `make` puts on the thread stays visible
    /// to it. Natively the thread is a blocking thread of the runtime; on
    /// wasm32 the one thread there is.
    pub fn spawn_pinned<M, Fut>(&self, make: M) -> Task<Fut::Output>
    where
        M: FnOnce() -> Fut + MaybeSend + 'static,
        Fut: Future + 'static,
        Fut::Output: MaybeSend + 'static,
    {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let rt = self.shared.rt.clone();
            self.spawn_blocking(move || rt.drive(make()))
        }
        #[cfg(target_arch = "wasm32")]
        {
            self.spawn(async move { make().await })
        }
    }

    /// Runs `future` as a long-running service, which stops when this
    /// handle's runtime or scope stops accepting.
    pub fn spawn_service<F>(&self, future: F) -> Service
    where
        F: Future<Output = ()> + MaybeSend + 'static,
    {
        Service::new(self.spawn(future.or(self.closing())))
    }

    /// Completes after `duration` on the runtime's clock, or early when the
    /// runtime is stopping.
    pub fn sleep(&self, duration: Duration) -> impl Future<Output = ()> + 'static {
        let mut timer = Timer::after(&self.shared, Instant::now(), duration);
        poll_fn(move |cx| timer.poll(cx).map(drop))
    }

    /// `future`'s value, or [`EdgeError::Deadline`] once `limit` passes
    /// first. The future is dropped at the deadline.
    pub fn timeout<F>(
        &self,
        limit: Duration,
        future: F,
    ) -> impl Future<Output = Result<F::Output, EdgeError>>
    where
        F: Future,
    {
        let mut timer = Timer::after(&self.shared, Instant::now(), limit);
        async move {
            let mut future = pin!(future);
            poll_fn(|cx| {
                if let Poll::Ready(value) = future.as_mut().poll(cx) {
                    return Poll::Ready(Ok(value));
                }
                timer.poll_limit(cx, limit).map(Err)
            })
            .await
        }
    }

    /// Runs `future` to its end on the calling thread, which it blocks.
    ///
    /// For a thread outside the runtime: a program's `main`, a C caller, a
    /// synchronous trait whose work is network I/O. Inside a nuclei task the
    /// wait nests: the thread's shard keeps running its other tasks. A panic
    /// in `future` is returned as [`EdgeError::Panicked`].
    #[cfg(not(target_arch = "wasm32"))]
    pub fn block_on<F>(&self, future: F) -> Result<F::Output, EdgeError>
    where
        F: Future,
    {
        let Some(_permit) = self.admit() else {
            return Err(EdgeError::Closed);
        };
        let rt = &self.shared.rt;
        contain(
            &self.shared.stats,
            crate::platform::wait_here(|| rt.drive(future)),
        )
    }

    /// A handle whose work can be stopped and drained apart from the rest
    /// of the runtime, for one component's requests.
    pub fn scope(&self) -> Scope {
        Scope {
            edge: Self::new(Arc::clone(&self.shared), self.gate.child()),
        }
    }

    /// Completes once this handle stops accepting work: when its runtime,
    /// or a scope it came from, begins to shut down. A server stops
    /// accepting connections on it.
    pub fn closing(&self) -> impl Future<Output = ()> + 'static {
        Arc::clone(&self.gate).closing()
    }

    /// Whether this handle stopped accepting work.
    pub fn is_closed(&self) -> bool {
        self.gate.is_closed()
    }

    /// The CPUs the runtime's threads run on, held for them alone; empty
    /// when they run unpinned.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn cpus(&self) -> &workers::topology::CpuSet {
        &self.shared.rt.cpus
    }

    /// The runtime's counters, with this handle's work in flight.
    pub fn stats(&self) -> EdgeStats {
        self.shared.stats.snapshot(self.gate.in_flight())
    }

    /// How long a waiter waits on a completion unless it sets its own
    /// deadline.
    pub fn deadline(&self) -> Duration {
        self.shared.deadline
    }
}

/// Two handles are equal when they admit work through the same gate: the
/// same runtime, or the same scope of it.
impl PartialEq for Edge {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.gate, &other.gate)
    }
}

impl Eq for Edge {}

impl fmt::Debug for Edge {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Edge")
            .field("closed", &self.is_closed())
            .field("stats", &self.stats())
            .finish()
    }
}

/// One component's share of the runtime: its work is admitted through its
/// own [`Edge`], and it stops accepting and drains on its own, while the
/// runtime and every other scope go on.
pub struct Scope {
    edge: Edge,
}

impl Scope {
    /// The handle this scope's work is submitted through.
    pub fn edge(&self) -> &Edge {
        &self.edge
    }

    /// Stops accepting work through this scope.
    pub fn close(&self) {
        self.edge.gate.close();
    }

    /// Stops accepting work through this scope, then waits up to `within`
    /// for the work it admitted to finish.
    pub async fn drain(&self, within: Duration) -> Drained {
        self.close();
        drain_gate(&self.edge.gate, &self.edge.shared, within).await
    }
}

/// What a drain left behind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Drained {
    /// Admitted work still running when the drain's deadline passed.
    pub remaining: usize,
}

impl Drained {
    /// Whether every admitted piece of work finished in time.
    pub fn is_complete(&self) -> bool {
        self.remaining == 0
    }
}

/// Waits up to `within` for what `gate` admitted to finish.
pub(crate) async fn drain_gate(gate: &Gate, shared: &Arc<Shared>, within: Duration) -> Drained {
    let mut timer = Timer::after(shared, Instant::now(), within);
    let mut drained = pin!(gate.drained());
    poll_fn(|cx| {
        if drained.as_mut().poll(cx).is_ready() {
            return Poll::Ready(());
        }
        timer.poll(cx).map(drop)
    })
    .await;
    Drained {
        remaining: gate.in_flight(),
    }
}
