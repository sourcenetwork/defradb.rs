//! Where the P2P layer's network work runs: on the network edge it was given
//! (`edge::Edge`), explicitly, and never on a worker of the data path's pool,
//! which has no reactor.
//!
//! A [`Network`] is handed to every component that spawns network work,
//! waits on a network deadline or sleeps before a retry. With an edge it
//! goes through the edge. Without one (a test that runs the P2P layer on its
//! own runtime) it uses the runtime of the task it is called from; that
//! fallback lives here, in the network layer, and nowhere on the data path.
//!
//! [`Network::hand_off`] is the crossing the data path uses: a call made on
//! a pool worker runs its network part on the edge and awaits it there;
//! anywhere else the call runs in place.

use std::future::Future;
use std::time::Duration;

use defra_core::thread_bounds::{MaybeBoxFuture, MaybeSend};

/// The network work of one component: the edge it runs on, when it has one.
#[derive(Clone, Default)]
pub struct Network {
    edge: Option<edge::Edge>,
}

/// A network deadline passed before the work finished.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Elapsed;

impl std::fmt::Display for Elapsed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("the network deadline passed")
    }
}

impl std::error::Error for Elapsed {}

impl Network {
    /// Network work on `edge`.
    pub fn on(edge: edge::Edge) -> Self {
        Self { edge: Some(edge) }
    }

    /// The edge network work runs on, when there is one.
    pub fn edge(&self) -> Option<&edge::Edge> {
        self.edge.as_ref()
    }

    /// Runs `future` as detached network work.
    pub fn spawn<F>(&self, future: F)
    where
        F: Future<Output = ()> + MaybeSend + 'static,
    {
        match &self.edge {
            Some(edge) => drop(edge.spawn(future)),
            None => crate::rt::spawn(future).detach(),
        }
    }

    /// Starts `future` as network work now and hands its output back when
    /// awaited; dropped unawaited, the work runs on. `None` when the network
    /// refused it (it is shutting down) or it panicked.
    pub fn run<F>(&self, future: F) -> MaybeBoxFuture<'static, Option<F::Output>>
    where
        F: Future + MaybeSend + 'static,
        F::Output: MaybeSend + 'static,
    {
        let (done, output) = regolith::sync::unbounded();
        self.spawn(async move {
            let _ = done.try_send(future.await);
        });
        Box::pin(async move { output.recv_async().await })
    }

    /// Completes after `duration`, on the edge's clock when there is one.
    pub fn sleep(&self, duration: Duration) -> MaybeBoxFuture<'static, ()> {
        match &self.edge {
            Some(edge) => Box::pin(edge.sleep(duration)),
            None => Box::pin(crate::rt::time::sleep(duration)),
        }
    }

    /// `future`'s output, or [`Elapsed`] once `limit` passes first.
    pub async fn timeout<F>(&self, limit: Duration, future: F) -> Result<F::Output, Elapsed>
    where
        F: Future,
    {
        match &self.edge {
            Some(edge) => edge.timeout(limit, future).await.map_err(|_| Elapsed),
            None => crate::rt::time::timeout(limit, future)
                .await
                .map_err(|_| Elapsed),
        }
    }

    /// Runs the network work `make` builds where network work runs, and
    /// hands its output back: on the edge when called on a pool worker (the
    /// work needs a reactor the worker has none of), in place anywhere else.
    /// `None` when the edge refused it (it is shutting down) or it panicked.
    pub async fn hand_off<F, Fut>(&self, make: F) -> Option<Fut::Output>
    where
        F: FnOnce() -> Fut + MaybeSend + 'static,
        Fut: Future + MaybeSend + 'static,
        Fut::Output: MaybeSend + 'static,
    {
        match self.crossing() {
            Some(_) => self.run(async move { make().await }).await,
            None => Some(make().await),
        }
    }

    /// The edge to cross to: the caller is a pool worker and an edge exists.
    pub fn crossing(&self) -> Option<&edge::Edge> {
        #[cfg(not(target_arch = "wasm32"))]
        if workers::on_worker() {
            return self.edge.as_ref();
        }
        // A browser's one worker is the page's thread, which runs the event
        // loop network work needs: nothing to cross.
        None
    }
}

impl std::fmt::Debug for Network {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Network")
            .field("edge", &self.edge.is_some())
            .finish()
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;

    #[test]
    fn work_handed_off_from_a_worker_runs_on_the_edge() {
        let edge = edge::testing::edge();
        let network = Network::on(edge.clone());
        let pool = workers::Pool::start(workers::Config::unplaced(1)).unwrap();
        let on_worker = network.clone();
        let ran_on_worker = pool
            .submit_and_wait(workers::Target::Worker(0), move || async move {
                on_worker.hand_off(|| async { workers::on_worker() }).await
            })
            .unwrap();
        assert_eq!(
            ran_on_worker,
            Some(false),
            "the network part left the worker"
        );
        // Off the pool it runs in place.
        assert_eq!(
            edge.block_on(network.hand_off(|| async { 7 })).unwrap(),
            Some(7)
        );
        pool.shutdown_blocking().unwrap();
    }

    #[test]
    fn work_run_from_a_worker_hands_its_output_back() {
        let network = Network::on(edge::testing::edge());
        let pool = workers::Pool::start(workers::Config::unplaced(1)).unwrap();
        let ran = pool
            .submit_and_wait(workers::Target::Worker(0), move || async move {
                network.run(async { workers::on_worker() }).await
            })
            .unwrap();
        assert_eq!(ran, Some(false), "the work ran on the edge");
        pool.shutdown_blocking().unwrap();
    }

    #[test]
    fn a_deadline_and_a_sleep_run_on_the_edges_clock() {
        let edge = edge::testing::edge();
        let network = Network::on(edge.clone());
        let late = edge
            .block_on(network.timeout(Duration::from_millis(5), std::future::pending::<()>()))
            .unwrap();
        assert_eq!(late, Err(Elapsed));
        edge.block_on(network.sleep(Duration::from_millis(1)))
            .unwrap();
    }
}
