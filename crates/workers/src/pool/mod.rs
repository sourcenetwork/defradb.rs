//! The pool: its workers' threads, submission and admission, the blocking
//! pool, the stuck-poll monitor, the placement of every thread, and shutdown.
//!
//! [`Pool::start`] reads the topology, plans each worker's place on the
//! cores no other placed pool of the process holds, locks memory if asked,
//! starts one thread per worker (each pins itself, sets its memory policy
//! and priority, then builds its hook so what the hook allocates is
//! node-local), waits for all of them, and logs once what the OS granted.
//! [`Pool::driven`] is the same pool with one worker that the caller's thread
//! drives (wasm, single-core embedded).

mod background;
mod batch;
#[cfg(not(target_family = "wasm"))]
mod blocking;
#[cfg(all(test, not(target_family = "wasm")))]
mod datapath_tests;
mod deadline;
mod default;
mod detached;
mod driven;
mod env;
mod monitor;
mod placer;
mod report;
mod routing;
#[cfg(not(target_family = "wasm"))]
mod start;
#[cfg(all(test, not(target_family = "wasm")))]
mod tests;

use std::future::Future;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use regolith::sync::{Latch, LatchWait};

pub use background::{spawn_background, Background};
pub use deadline::{delay, delay_on, timeout, timeout_on, Delay};
pub use default::default_pool;
pub use detached::{run_on, spawn, spawn_detached};
pub use driven::Driver;
pub use env::PlacedEnv;
pub use monitor::{Monitor, MonitorDriver};
pub use placer::ThreadPlacement;
pub use report::{Report, TopologySummary, WorkerReport};
pub use routing::Target;

#[cfg(not(target_family = "wasm"))]
use blocking::Blocking;
use placer::Placer;

use crate::clock::Clock;
use crate::completion::Completion;
use crate::config::{Config, ThreadClass};
use crate::hook::{Hook, HookFactory};
use crate::topology::Topology;
use crate::worker::{Job, Parker, Signal, Tuning, WorkerShared, WorkerStats};
use crate::{Error, WorkerId};

/// Options for one submission.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Submit {
    /// The task's name in stuck and panic reports; the closure's type name
    /// when `None`.
    pub name: Option<&'static str>,
    /// The bytes the submission holds, charged against the worker's byte
    /// bound (a request body, say); the closure's own size when `None`.
    pub bytes: Option<usize>,
}

/// The pool's counters.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stats {
    /// Each worker's counters and load.
    pub workers: Vec<WorkerStats>,
    /// Jobs sent to `spawn_blocking` so far.
    pub blocking_spawned: u64,
    /// Blocking jobs waiting for a thread now.
    pub blocking_queued: usize,
}

/// What every thread of the pool shares.
pub(crate) struct PoolShared {
    pub(crate) workers: Box<[Arc<WorkerShared>]>,
    /// Worker indices per node, by node id.
    nodes: Vec<(usize, Vec<usize>)>,
    all: Vec<usize>,
    /// The node of each CPU id, for preferring the caller's node.
    cpu_node: Vec<Option<usize>>,
    pub(crate) stuck_after: Duration,
    pub(crate) clock: Arc<dyn Clock>,
    placer: Arc<Placer>,
    closing: AtomicBool,
    stopped: Latch,
    blocking_spawned: AtomicU64,
    #[cfg(not(target_family = "wasm"))]
    blocking: Option<Arc<Blocking>>,
}

impl PoolShared {
    /// The shared state of a pool with one worker per seat of `placer`'s
    /// plan, each woken through its parker.
    fn new(
        config: &Config,
        topology: &Topology,
        placer: Arc<Placer>,
        clock: Arc<dyn Clock>,
        parkers: Vec<Parker>,
    ) -> PoolShared {
        let seats = &placer.plan.seats;
        let workers: Box<[Arc<WorkerShared>]> = seats
            .iter()
            .zip(parkers)
            .enumerate()
            .map(|(index, (seat, parker))| {
                let signal = Arc::new(Signal::new(parker));
                let id = WorkerId {
                    index,
                    node: seat.node,
                };
                Arc::new(WorkerShared::new(
                    id,
                    signal,
                    config.inbox_items,
                    config.inbox_bytes,
                ))
            })
            .collect();
        let mut nodes: Vec<(usize, Vec<usize>)> = Vec::new();
        for (index, seat) in seats.iter().enumerate() {
            match nodes.iter_mut().find(|(node, _)| *node == seat.node) {
                Some((_, list)) => list.push(index),
                None => nodes.push((seat.node, vec![index])),
            }
        }
        nodes.sort_by_key(|(node, _)| *node);
        let mut cpu_node = Vec::new();
        for (node, core) in topology.cores() {
            for cpu in core.cpus.iter() {
                if cpu_node.len() <= cpu {
                    cpu_node.resize(cpu + 1, None);
                }
                cpu_node[cpu] = Some(node.id);
            }
        }
        PoolShared {
            all: (0..workers.len()).collect(),
            stopped: Latch::new(workers.len()),
            workers,
            nodes,
            cpu_node,
            stuck_after: config.stuck_after,
            clock,
            placer,
            closing: AtomicBool::new(false),
            blocking_spawned: AtomicU64::new(0),
            #[cfg(not(target_family = "wasm"))]
            blocking: None,
        }
    }

    /// Room for a job of `bytes` on a worker `target` allows, taken now
    /// without waiting; `None` when there is none.
    pub(crate) fn admit_now(
        &self,
        target: Target,
        bytes: usize,
    ) -> Result<Option<crate::worker::Admitted>, Error> {
        if self.closing.load(Ordering::Acquire) {
            return Err(Error::ShutDown);
        }
        Ok(self
            .route(target)?
            .try_admit(bytes)
            .or_else(|| self.admit_anywhere(target, bytes)))
    }

    fn tuning(config: &Config, monitored: bool) -> Tuning {
        Tuning {
            inbox_batch: config.inbox_batch,
            run_batch: config.run_batch,
            io_budget: config.io_budget,
            idle_spin: config.idle_spin,
            stuck_after: config.stuck_after,
            monitored,
        }
    }

    /// Stops accepting work and tells every worker to finish and stop.
    fn shutdown(&self) {
        if self.closing.swap(true, Ordering::AcqRel) {
            return;
        }
        self.workers.iter().for_each(|worker| worker.shutdown());
        #[cfg(not(target_family = "wasm"))]
        if let Some(blocking) = &self.blocking {
            blocking.close();
        }
    }

    fn abandon(&self) {
        self.workers.iter().for_each(|worker| worker.abandon());
    }
}

/// Builds a worker's hook on the calling thread; a factory that panics is
/// an error, not a dead thread.
fn build_hook(factory: Option<&HookFactory>, id: WorkerId) -> Result<Option<Box<dyn Hook>>, Error> {
    let Some(factory) = factory else {
        return Ok(None);
    };
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| factory(id)))
        .map(Some)
        .map_err(|payload| {
            let message = crate::worker::panic_message(payload.as_ref());
            Error::Spawn(format!("the hook factory panicked: {message}"))
        })
}

/// A handle to the pool, cheap to clone. When the last handle is dropped
/// without a shutdown, the workers are told to stop at once and what they
/// had not finished fails with [`Error::ShutDown`].
#[derive(Clone)]
pub struct Pool {
    handle: Arc<Handle>,
}

struct Handle {
    shared: Arc<PoolShared>,
    report: Report,
    shutdown_deadline: Duration,
    #[cfg(not(target_family = "wasm"))]
    threads: regolith::sync::ArrayQueue<std::thread::JoinHandle<()>>,
}

impl Drop for Handle {
    fn drop(&mut self) {
        self.shared.shutdown();
        self.shared.abandon();
    }
}

impl Pool {
    fn from_parts(
        shared: Arc<PoolShared>,
        report: Report,
        config: &Config,
        #[cfg(not(target_family = "wasm"))] threads: Vec<std::thread::JoinHandle<()>>,
    ) -> Pool {
        report.log();
        #[cfg(not(target_family = "wasm"))]
        let threads = {
            let queue = regolith::sync::ArrayQueue::new(threads.len().max(1));
            threads.into_iter().for_each(|thread| {
                let _ = queue.push(thread);
            });
            queue
        };
        Pool {
            handle: Arc::new(Handle {
                shared,
                report,
                shutdown_deadline: config.shutdown_deadline,
                #[cfg(not(target_family = "wasm"))]
                threads,
            }),
        }
    }

    /// Whether the pool has begun to shut down: it takes no new work.
    pub fn is_closing(&self) -> bool {
        self.handle.shared.closing.load(Ordering::Acquire)
    }

    /// How many workers the pool runs.
    pub fn workers(&self) -> usize {
        self.handle.shared.workers.len()
    }

    /// The NUMA nodes that hold a worker, by id.
    pub fn nodes(&self) -> Vec<usize> {
        self.handle
            .shared
            .nodes
            .iter()
            .map(|(node, _)| *node)
            .collect()
    }

    /// What the pool asked the OS for at start and what it was granted.
    pub fn report(&self) -> &Report {
        &self.handle.report
    }

    /// The pool's counters now.
    pub fn stats(&self) -> Stats {
        let shared = &self.handle.shared;
        #[cfg(not(target_family = "wasm"))]
        let blocking_queued = shared
            .blocking
            .as_ref()
            .map_or(0, |blocking| blocking.queued());
        #[cfg(target_family = "wasm")]
        let blocking_queued = 0;
        Stats {
            workers: shared.workers.iter().map(|worker| worker.stats()).collect(),
            blocking_spawned: shared.blocking_spawned.load(Ordering::Relaxed),
            blocking_queued,
        }
    }

    /// Runs the future `make` builds on the worker `target` names, and
    /// returns its completion once the worker admitted it. `make` crosses to
    /// the worker; the future it builds there never moves, so it need not be
    /// `Send`. Waits while the worker is full; never queues past its bounds.
    ///
    /// ```no_run
    /// # async fn demo(pool: workers::Pool) -> Result<(), workers::Error> {
    /// let doc_hash = 42;
    /// let done = pool
    ///     .submit(workers::Target::Key(doc_hash), move || async move { doc_hash * 2 })
    ///     .await?;
    /// assert_eq!(done.await?, 84);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn submit<F, Fut>(
        &self,
        target: Target,
        make: F,
    ) -> Result<Completion<Fut::Output>, Error>
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future + 'static,
        Fut::Output: Send + 'static,
    {
        self.submit_with(target, Submit::default(), make).await
    }

    /// [`submit`](Self::submit) with a name and a byte count.
    pub async fn submit_with<F, Fut>(
        &self,
        target: Target,
        options: Submit,
        make: F,
    ) -> Result<Completion<Fut::Output>, Error>
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future + 'static,
        Fut::Output: Send + 'static,
    {
        let shared = &self.handle.shared;
        let bytes = options.bytes.unwrap_or(std::mem::size_of::<F>());
        let admitted = match shared.admit_now(target, bytes)? {
            Some(admitted) => admitted,
            None => shared.route(target)?.admit(bytes).await,
        };
        Ok(Self::launch(target, options, admitted, make))
    }

    /// [`submit_with`](Self::submit_with) that never waits, for a caller that
    /// cannot (a destructor, a synchronous API): [`Error::Full`] when no worker
    /// `target` allows has room now.
    pub fn try_submit_with<F, Fut>(
        &self,
        target: Target,
        options: Submit,
        make: F,
    ) -> Result<Completion<Fut::Output>, Error>
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future + 'static,
        Fut::Output: Send + 'static,
    {
        let bytes = options.bytes.unwrap_or(std::mem::size_of::<F>());
        let admitted = self
            .handle
            .shared
            .admit_now(target, bytes)?
            .ok_or(Error::Full)?;
        Ok(Self::launch(target, options, admitted, make))
    }

    /// Hands an admitted job to the worker its admission belongs to.
    pub(crate) fn launch<F, Fut>(
        target: Target,
        options: Submit,
        admitted: crate::worker::Admitted,
        make: F,
    ) -> Completion<Fut::Output>
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future + 'static,
        Fut::Output: Send + 'static,
    {
        let worker = admitted.worker().clone();
        let (job, completion) = Self::job(target, options, admitted, make);
        worker.push(job);
        completion
    }

    /// The job that runs `make` with its admission, not handed over yet.
    pub(crate) fn job<F, Fut>(
        target: Target,
        options: Submit,
        admitted: crate::worker::Admitted,
        make: F,
    ) -> (Job, Completion<Fut::Output>)
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future + 'static,
        Fut::Output: Send + 'static,
    {
        let key = match target {
            Target::Key(key) => Some(key),
            _ => None,
        };
        let name = options.name.unwrap_or(std::any::type_name::<F>());
        Job::new(
            make,
            name,
            key,
            matches!(target, Target::Worker(_)),
            admitted,
        )
    }

    /// Submits from a plain thread and waits for the result: for the C FFI, a
    /// CLI's main, a test. Errors with [`Error::OnWorker`] on a worker.
    #[cfg(not(target_family = "wasm"))]
    pub fn submit_and_wait<F, Fut>(&self, target: Target, make: F) -> Result<Fut::Output, Error>
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future + 'static,
        Fut::Output: Send + 'static,
    {
        crate::completion::block_on(self.submit(target, make))??.wait()
    }

    /// Runs code that has to block on one of the blocking pool's
    /// lowest-priority threads, off the workers. Waits while its queue is
    /// full. A pool with no threads (wasm) runs it on its worker. Counted in
    /// [`Stats::blocking_spawned`].
    pub async fn spawn_blocking<F, T>(&self, f: F) -> Result<Completion<T>, Error>
    where
        F: FnOnce() -> T + Send + 'static,
        T: Send + 'static,
    {
        let shared = &self.handle.shared;
        shared.blocking_spawned.fetch_add(1, Ordering::Relaxed);
        #[cfg(not(target_family = "wasm"))]
        if let Some(blocking) = &shared.blocking {
            return blocking.spawn(f).await;
        }
        let name = std::any::type_name::<F>();
        self.submit_with(
            Target::Worker(0),
            Submit {
                name: Some(name),
                bytes: None,
            },
            move || async move { f() },
        )
        .await
    }

    /// Places the calling thread, one the pool did not start (a network
    /// thread, a periodic-job thread), as a thread of `class`: on one node's
    /// spare CPUs, in turn, at the class's priority.
    pub fn place_current_thread(&self, class: ThreadClass) -> ThreadPlacement {
        self.handle.shared.placer.place(class)
    }

    /// `inner`, with the threads regolith starts through it (compaction, log
    /// writeback) placed as [`ThreadClass::Compaction`] threads.
    pub fn env(&self, inner: Arc<dyn regolith::Env>) -> Arc<dyn regolith::Env> {
        Arc::new(PlacedEnv::new(inner, self.handle.shared.placer.clone()))
    }

    /// Stops accepting work and tells every worker to finish what it admitted
    /// and stop. Returns at once: await [`stopped`](Self::stopped) with a
    /// deadline, or call [`shutdown_blocking`](Self::shutdown_blocking).
    pub fn shutdown(&self) {
        self.handle.shared.shutdown();
    }

    /// Completes once every worker has stopped.
    pub fn stopped(&self) -> LatchWait<'_> {
        self.handle.shared.stopped.wait()
    }

    /// Tells every worker to stop now: what it has not finished fails with
    /// [`Error::ShutDown`]. A worker inside a poll stops when the poll returns.
    pub fn abandon(&self) {
        self.handle.shared.abandon();
    }

    /// [`shutdown`](Self::shutdown), then completes once every worker has
    /// stopped or the configured deadline passed, from any executor. Past
    /// the deadline it abandons what is left and returns
    /// [`Error::TimedOut`]. Called on a worker it returns
    /// [`Error::OnWorker`] at once: a worker cannot wait for itself to stop.
    pub async fn stop(&self) -> Result<(), Error> {
        if crate::worker::on_worker() {
            return Err(Error::OnWorker);
        }
        self.shutdown();
        match crate::timeout(self.handle.shutdown_deadline, self.stopped()).await {
            Ok(()) => Ok(()),
            Err(error) => {
                self.abandon();
                Err(error)
            }
        }
    }

    /// [`shutdown`](Self::shutdown), then waits on a plain thread up to the
    /// configured deadline. Past it, abandons what is left and returns
    /// [`Error::TimedOut`]; workers still inside a poll are left to stop when
    /// it returns.
    #[cfg(not(target_family = "wasm"))]
    pub fn shutdown_blocking(&self) -> Result<(), Error> {
        self.shutdown();
        let result = match crate::completion::block_on_timeout(
            self.stopped(),
            self.handle.shutdown_deadline,
        ) {
            Ok(()) => Ok(()),
            Err(Error::TimedOut) => {
                self.abandon();
                Err(Error::TimedOut)
            }
            Err(other) => return Err(other),
        };
        self.join_finished();
        result
    }

    /// Joins every pool thread that has stopped.
    #[cfg(not(target_family = "wasm"))]
    fn join_finished(&self) {
        let threads = &self.handle.threads;
        let mut running = Vec::new();
        while let Some(thread) = threads.pop() {
            if thread.is_finished() {
                let _ = thread.join();
            } else {
                running.push(thread);
            }
        }
        running.into_iter().for_each(|thread| {
            let _ = threads.push(thread);
        });
        if let Some(blocking) = &self.handle.shared.blocking {
            blocking.join_finished();
        }
    }
}

impl std::fmt::Debug for Pool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Pool")
            .field("workers", &self.workers())
            .finish_non_exhaustive()
    }
}
