//! The runtime a service runs its round trips on, and the clock its
//! deadlines and retries read.

use std::fmt;
use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use defra_core::thread_bounds::{MaybeBoxFuture, MaybeSend, MaybeSendSync};
use workers::Clock;

#[cfg(target_arch = "wasm32")]
use n0_future::time::Instant;
#[cfg(not(target_arch = "wasm32"))]
use nuclei::time::Instant;

/// Where round trips run: network work spawned apart from the tasks that
/// wait on it, and timers for deadlines and backoff.
///
/// The network edge ([`edge::Edge`]) is the implementation a node passes at
/// construction; a component given none reaches the process's on-demand
/// runtime ([`Runtime::on_demand`]).
pub trait Network: MaybeSendSync + 'static {
    /// Runs `work` to its end, whoever waits on it. A network that cannot
    /// run it drops it, which settles its keys as closed.
    fn spawn(&self, work: MaybeBoxFuture<'static, ()>);

    /// Completes after `duration`.
    fn sleep(&self, duration: Duration) -> MaybeBoxFuture<'static, ()>;
}

/// The network edge: round trips run as its network tasks, so they stop
/// with it, and its timers time deadlines and backoff.
impl Network for edge::Edge {
    fn spawn(&self, work: MaybeBoxFuture<'static, ()>) {
        // Dropping the completion leaves the work running. A round trip
        // refused at shutdown settles its keys as closed when dropped.
        drop(edge::Edge::spawn(self, work));
    }

    fn sleep(&self, duration: Duration) -> MaybeBoxFuture<'static, ()> {
        Box::pin(edge::Edge::sleep(self, duration))
    }
}

/// The process's on-demand network runtime ([`edge::on_demand`]), started
/// by the first round trip that needs it. A runtime that cannot start runs
/// nothing: the round trip is dropped, which settles its keys as closed, and
/// the reason is logged.
#[cfg(not(target_arch = "wasm32"))]
struct OnDemand;

#[cfg(not(target_arch = "wasm32"))]
impl OnDemand {
    fn edge() -> Option<&'static edge::Edge> {
        edge::on_demand()
            .inspect_err(|error| tracing::error!(%error, "the network runtime did not start"))
            .ok()
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Network for OnDemand {
    fn spawn(&self, work: MaybeBoxFuture<'static, ()>) {
        if let Some(edge) = Self::edge() {
            Network::spawn(edge, work);
        }
    }

    fn sleep(&self, duration: Duration) -> MaybeBoxFuture<'static, ()> {
        match Self::edge() {
            Some(edge) => Network::sleep(edge, duration),
            // Only a wait on work that never ran asks: its keys settle as
            // closed, which ends the wait first.
            None => Box::pin(std::future::pending()),
        }
    }
}

/// The runtime's monotonic clock: nuclei's natively (so a test that pauses
/// nuclei's time moves it too), the browser's on wasm.
#[derive(Debug)]
pub struct RuntimeClock {
    origin: Instant,
}

impl RuntimeClock {
    /// A clock whose origin is now.
    pub fn new() -> Self {
        Self {
            origin: Instant::now(),
        }
    }
}

impl Default for RuntimeClock {
    fn default() -> Self {
        Self::new()
    }
}

impl Clock for RuntimeClock {
    fn now_micros(&self) -> u64 {
        u64::try_from(self.origin.elapsed().as_micros()).unwrap_or(u64::MAX)
    }
}

/// Time since it was started, on the network runtime's clock: what a caller
/// measures its own wait with.
#[derive(Debug)]
pub struct Stopwatch(Instant);

impl Stopwatch {
    /// A stopwatch running from now.
    pub fn start() -> Self {
        Self(Instant::now())
    }

    /// Microseconds since it started.
    pub fn elapsed_micros(&self) -> u64 {
        u64::try_from(self.0.elapsed().as_micros()).unwrap_or(u64::MAX)
    }
}

/// A service's network and clock, passed together.
///
/// With a network (the node's edge, or the process's on-demand runtime)
/// round trips run as its tasks, shared by every caller that asks for a key
/// in flight, and timed by its timers. Inline a caller's round trips run in
/// the caller's own task: nothing is spawned and no timer is read, so it
/// never reaches for a runtime. Its keys are batched and cached as usual but shared with
/// no other caller, a round trip is bounded only by the source's own
/// timeouts, and a retry follows a failure at once.
#[derive(Clone)]
pub struct Runtime {
    pub(crate) network: Option<Arc<dyn Network>>,
    pub(crate) clock: Arc<dyn Clock>,
}

impl Runtime {
    /// Round trips on `network`, time read from `clock` (a regolith `Env`'s,
    /// through [`workers::EnvClock`], where the node has one).
    pub fn new(network: Arc<dyn Network>, clock: Arc<dyn Clock>) -> Self {
        Self {
            network: Some(network),
            clock,
        }
    }

    /// The network edge `edge` and its runtime's clock.
    pub fn edge(edge: edge::Edge) -> Self {
        Self::new(Arc::new(edge), Arc::new(RuntimeClock::new()))
    }

    /// The process's network runtime, started by the first round trip that
    /// needs it ([`edge::on_demand`]): for a component given no edge, so it
    /// reaches the network all the same and a process that never does starts
    /// no threads. In a browser, where a request needs no runtime of its own,
    /// [`inline`](Self::inline).
    pub fn on_demand() -> Self {
        #[cfg(not(target_arch = "wasm32"))]
        return Self::new(Arc::new(OnDemand), Arc::new(RuntimeClock::new()));
        #[cfg(target_arch = "wasm32")]
        Self::inline()
    }

    /// No network: each caller runs its own round trips in its own task.
    pub fn inline() -> Self {
        Self {
            network: None,
            clock: Arc::new(RuntimeClock::new()),
        }
    }

    /// The same network with `clock`.
    pub fn with_clock(mut self, clock: Arc<dyn Clock>) -> Self {
        self.clock = clock;
        self
    }

    pub(crate) fn now(&self) -> u64 {
        self.clock.now_micros()
    }

    /// Runs the round trip `make` builds where round trips run, and hands
    /// its output back: on the network when called on a pool worker (the
    /// round trip needs a reactor the worker has none of), in place anywhere
    /// else or inline. `None` when the network refused it (it is shutting
    /// down) or it panicked.
    pub async fn hand_off<F, Fut>(&self, make: F) -> Option<Fut::Output>
    where
        F: FnOnce() -> Fut + MaybeSend + 'static,
        Fut: Future + MaybeSend + 'static,
        Fut::Output: MaybeSend + 'static,
    {
        // A browser's one worker is the page's thread, which runs the event
        // loop round trips need: nothing to cross.
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(network) = self.network.as_ref().filter(|_| workers::on_worker()) {
            let (done, output) = regolith::sync::unbounded();
            network.spawn(Box::pin(async move {
                let _ = done.try_send(make().await);
            }));
            return output.recv_async().await;
        }
        Some(make().await)
    }
}

impl fmt::Debug for Runtime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Runtime")
            .field("inline", &self.network.is_none())
            .field("clock", &self.clock)
            .finish_non_exhaustive()
    }
}
