//! The I/O a pool worker does for the stores it reads and writes.
//!
//! Each worker of a [`workers::Pool`] built with [`with_worker_io`] holds one
//! regolith `IoQueue` per database it touches, opened on its own thread the
//! first time a transaction there reaches that database, and polls every one
//! of them between tasks ([`WorkerIo`], the pool's hook). A transaction that
//! starts on such a worker reads in `ReadMode::CacheOnly` on that queue, so a
//! block the cache does not hold suspends the task on its `IoWait` instead of
//! the thread on the device, and commits with `commit_nowait`, whose ticket
//! the same queue completes. Nothing is shared with another thread: the
//! queues live in the worker's thread-local registry ([`registry`]) and only
//! that worker polls them.
//!
//! A caller off the workers of a store's pool (a network task not yet routed
//! to the pool) borrows one worker's queue for that store ([`StoreIo`]): its
//! reads miss into that queue and its commits complete there, so it never
//! reads the device or waits for a sync on its own thread. Each such call is
//! counted (`off_pool_calls`), because it is a request that was not routed.
//! A store with no pool, or whose pool has stopped (a test, a one-shot CLI
//! tool, the last close), reads in `ReadMode::Blocking` and commits
//! synchronously, as a synchronous caller asks for.
//!
//! [`settle`] waits out what a non-blocking call hands back, and [`drive`]
//! runs work whose waits are on a queue of its own from the task that awaits
//! it, for a caller that is not a worker.

mod drive;
mod registry;
mod settle;

use std::sync::Arc;

use regolith::{IoQueue, OptimisticTransactionDb, QueueId, ReadMode};

pub(crate) use drive::drive;
pub(crate) use settle::{readied, ready_after, settle, settle_after, settled, wait_out};

/// The pool hook that polls this worker's regolith queues. Built on the
/// worker's own thread by the factory [`with_worker_io`] installs; while it
/// lives, transactions on the thread use the thread's queues.
pub struct WorkerIo {
    _bound: registry::Bound,
}

impl workers::Hook for WorkerIo {
    fn poll(&mut self, budget: usize) -> workers::HookProgress {
        registry::poll(budget)
    }

    fn idle(&mut self, waker: &std::task::Waker) {
        registry::idle(waker);
    }
}

/// `config` with the hook that gives each worker its regolith queues: what
/// every pool that runs the data path is started with.
///
/// ```no_run
/// let pool = workers::Pool::start(storage::with_worker_io(workers::Config::default()))?;
/// let options = storage::RegolithStoreOptions::default().placed_on(&pool);
/// let store = storage::RegolithStore::open_with_options("data", options)?;
/// # let _ = store;
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn with_worker_io(config: workers::Config) -> workers::Config {
    let hook: workers::HookFactory = Arc::new(|_worker| {
        Box::new(WorkerIo {
            _bound: registry::Bound::here(),
        })
    });
    workers::Config {
        hook: Some(hook),
        ..config
    }
}

/// Starts the pool a database's data path runs on: `config` with the hook
/// that gives each worker its regolith queues ([`with_worker_io`]), and with
/// `monitor` (the network edge's timer) running its stuck-poll scan when one
/// is given. What every node builder starts its pool with; it becomes the
/// process's default pool ([`workers::Pool::make_default`]), so work
/// submitted without a pool runs on it too.
#[cfg(not(target_arch = "wasm32"))]
pub fn start_pool(
    config: workers::Config,
    monitor: Option<workers::MonitorDriver>,
) -> Result<workers::Pool, workers::Error> {
    let config = workers::Config {
        monitor: monitor.or(config.monitor.clone()),
        ..config
    };
    let pool = workers::Pool::start(with_worker_io(config))?;
    pool.make_default();
    Ok(pool)
}

/// The calling worker's queue for `db`, opened on first use; `None` off a
/// worker that polls regolith queues.
pub(super) fn queue_for(db: &Arc<OptimisticTransactionDb>) -> Option<QueueId> {
    registry::queue_for(db)
}

/// Where one store's calls do their I/O: on the calling worker's queue, on a
/// queue borrowed from the store's pool when the caller is off its workers,
/// or blocking when the store has no running pool.
pub(super) struct StoreIo {
    pool: Option<workers::Pool>,
    /// The queue off-pool callers borrow, opened on first use on the
    /// least-loaded worker, which polls it among its own.
    borrowed: regolith::sync::OnceCell<QueueId>,
    /// Queues a caller with no worker to poll for it drives itself ([`drive`]),
    /// kept between its calls: opening one lists it with the engine and
    /// dropping one takes it off again, which each call would otherwise pay.
    /// At most one per thread that can run at once stays idle.
    idle: regolith::sync::ArrayQueue<IoQueue>,
    stats: crate::backends::TransactionStatsHandle,
}

impl StoreIo {
    pub(super) fn new(
        pool: Option<workers::Pool>,
        stats: crate::backends::TransactionStatsHandle,
    ) -> Self {
        let threads = std::thread::available_parallelism().map_or(1, usize::from);
        Self {
            pool,
            borrowed: regolith::sync::OnceCell::new(),
            idle: regolith::sync::ArrayQueue::new(threads),
            stats,
        }
    }

    /// A queue of the caller's own to drive, idle: one kept from an earlier
    /// call, or a new one on `db`.
    pub(super) fn own_queue(&self, db: &OptimisticTransactionDb) -> IoQueue {
        self.idle.pop().unwrap_or_else(|| db.db().io_queue())
    }

    /// Keeps `queue`, driven to the end of its caller's call, for the next
    /// caller: unless it still holds a read, or enough are kept already.
    pub(super) fn keep_queue(&self, queue: IoQueue) {
        if queue.pending_bytes() == 0 {
            let _ = self.idle.push(queue);
        }
    }

    /// The queue a call named `call` on `db` reads and commits through.
    pub(super) async fn queue(
        &self,
        db: &Arc<OptimisticTransactionDb>,
        call: &'static str,
    ) -> Option<QueueId> {
        if let Some(queue) = queue_for(db) {
            return Some(queue);
        }
        let pool = self.pool.as_ref().filter(|pool| !pool.is_closing())?;
        self.stats.record_off_pool(call);
        if let Some(queue) = self.borrowed.get() {
            return Some(*queue);
        }
        let on_worker = Arc::clone(db);
        let opened = pool
            .submit(workers::Target::LeastLoaded, move || async move {
                queue_for(&on_worker)
            })
            .await
            .ok()?
            .await
            .ok()??;
        // A racing caller may have set one first; both stay polled, one is
        // used from now on.
        let _ = self.borrowed.set(opened);
        self.borrowed.get().copied()
    }

    /// How a handle a call named `call` opens on `db` reads.
    pub(super) async fn read_mode(
        &self,
        db: &Arc<OptimisticTransactionDb>,
        call: &'static str,
    ) -> ReadMode {
        self.queue(db, call)
            .await
            .map_or(ReadMode::Blocking, ReadMode::CacheOnly)
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests;
