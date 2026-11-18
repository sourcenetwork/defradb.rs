//! One-worker mode driven by the caller's thread: the wasm main thread, a
//! single-core embedded target, an application's own event loop. The same
//! [`Pool`] API submits to it; the caller's thread runs the worker through a
//! [`Driver`] whenever the pool wakes it. Nothing needs a second thread.

use std::sync::Arc;
use std::task::Waker;
use std::time::Duration;

use super::placer::{Placer, ThreadPlacement};
use super::report::{Report, TopologySummary, WorkerReport};
use super::{build_hook, Pool, PoolShared};
use crate::config::{Config, MemoryPolicy};
use crate::os::{self, Outcome};
use crate::placement::{Plan, Seat};
use crate::topology::{CpuSet, Topology};
use crate::worker::{Parker, Worker};
use crate::{clock, Error, WorkerId};

/// The caller's side of a driven pool: runs its one worker on the calling
/// thread. Not `Send`: the worker's tasks live on this thread. Dropping it
/// stops the worker, failing what it had not finished.
pub struct Driver {
    worker: Option<Worker>,
    shared: Arc<PoolShared>,
    parks: bool,
}

impl Pool {
    /// A pool of one worker that the calling thread drives. `waker` is woken
    /// whenever the worker has something to do (a submission, a task woken
    /// from elsewhere, an I/O completion of its hook); the caller then calls
    /// [`Driver::turn`] until it returns 0, and again within
    /// [`Driver::next_due`] for timers. The thread is not re-placed or
    /// re-prioritized: it is the caller's.
    pub fn driven(config: Config, waker: Waker) -> Result<(Pool, Driver), Error> {
        drive(config, Parker::Waker(waker), false)
    }

    /// [`driven`](Pool::driven) for a thread that runs nothing else (a
    /// single-core embedded main): [`Driver::block_on`] parks it while the
    /// worker has nothing to do.
    #[cfg(not(target_family = "wasm"))]
    pub fn driven_here(config: Config) -> Result<(Pool, Driver), Error> {
        let parker = Parker::Thread(regolith::sync::OnceCell::new());
        drive(config, parker, true)
    }
}

fn drive(config: Config, parker: Parker, parks: bool) -> Result<(Pool, Driver), Error> {
    config.validate()?;
    if crate::worker::on_worker() {
        return Err(Error::OnWorker);
    }
    let clock = config
        .clock
        .clone()
        .or_else(clock::platform)
        .ok_or_else(|| Error::Config("this target has no clock; set Config::clock".into()))?;
    let id = WorkerId { index: 0, node: 0 };
    let hook = build_hook(config.hook.as_ref(), id)?;
    let topology = Topology::detect();
    let plan = Plan {
        seats: vec![Seat {
            node: 0,
            cpus: CpuSet::new(),
        }],
        background: Vec::new(),
        oversubscribed: false,
        notes: vec!["one worker, driven by the caller's thread".to_owned()],
    };
    let placer = Arc::new(Placer::new(
        plan,
        CpuSet::new(),
        config.priorities.clone(),
        MemoryPolicy::Off,
    ));
    #[cfg_attr(target_family = "wasm", allow(unused_mut))]
    let mut shared = PoolShared::new(
        &config,
        &topology,
        placer.clone(),
        clock.clone(),
        vec![parker],
    );
    #[cfg(not(target_family = "wasm"))]
    {
        shared.blocking = Some(super::blocking::Blocking::start(
            config.blocking_threads,
            config.blocking_queue,
            &config.thread_name,
            placer,
        )?);
    }
    let shared = Arc::new(shared);
    let worker_shared = shared.workers[0].clone();
    #[cfg(not(target_family = "wasm"))]
    worker_shared.signal.bind_current_thread();
    let worker = Worker::new(
        worker_shared,
        clock,
        PoolShared::tuning(&config, config.monitor.is_some()),
        hook,
    )?;
    if let Some(driver) = &config.monitor {
        driver(super::monitor::Monitor::of(&shared));
    }
    let caller = |what: &str| Outcome {
        asked: what.to_owned(),
        granted: what.to_owned(),
        fell_back: None,
    };
    let placement = ThreadPlacement {
        pinning: caller("the caller's thread"),
        memory_policy: caller("the caller's"),
        priority: caller("the caller's"),
    };
    let report = Report {
        topology: TopologySummary::of(&topology),
        workers: vec![WorkerReport { id, placement }],
        memory_lock: os::lock_memory(os::system(), &config.memory_lock),
        notes: shared.placer.plan.notes.clone(),
    };
    let pool = Pool::from_parts(
        shared.clone(),
        report,
        &config,
        #[cfg(not(target_family = "wasm"))]
        Vec::new(),
    );
    Ok((
        pool,
        Driver {
            worker: Some(worker),
            shared,
            parks,
        },
    ))
}

impl Driver {
    /// Runs one round of the worker: its inbox, its ready tasks, its hook
    /// and its due timers. Returns how much it did; 0 means idle, and the
    /// pool's waker fires when there is more.
    pub fn turn(&mut self) -> usize {
        let Some(worker) = self.worker.as_mut() else {
            return 0;
        };
        worker.shared().signal.running();
        let done = worker.turn();
        if done == 0 && !worker.finished() {
            worker.rest();
        }
        done
    }

    /// How long until a timer needs a turn though nothing woke the pool.
    pub fn next_due(&self) -> Option<Duration> {
        self.worker.as_ref().and_then(Worker::wait_for)
    }

    /// Whether the pool was shut down and its worker has finished.
    pub fn finished(&self) -> bool {
        self.worker.as_ref().is_none_or(Worker::finished)
    }

    /// Runs `future` as a task on the worker and drives the worker until it
    /// completes, parking the thread while there is nothing to do. Only for a
    /// driver from [`Pool::driven_here`]; one woken through a caller's waker
    /// returns [`Error::Config`].
    #[cfg(not(target_family = "wasm"))]
    pub fn block_on<F>(&mut self, future: F) -> Result<F::Output, Error>
    where
        F: std::future::Future + 'static,
        F::Output: 'static,
    {
        use std::future::Future;
        use std::pin::Pin;
        use std::task::{Context, Poll};

        if !self.parks {
            return Err(Error::Config(
                "block_on needs a driver from Pool::driven_here".into(),
            ));
        }
        let worker = self.worker.as_mut().ok_or(Error::ShutDown)?;
        let mut done: crate::Completion<F::Output> = worker.spawn(future)?;
        loop {
            if done.is_finished() {
                return match Pin::new(&mut done).poll(&mut Context::from_waker(Waker::noop())) {
                    Poll::Ready(result) => result,
                    Poll::Pending => Err(Error::Consumed),
                };
            }
            if worker.turn() == 0 && !done.is_finished() {
                worker.idle();
            }
        }
    }
}

impl Drop for Driver {
    fn drop(&mut self) {
        drop(self.worker.take());
        self.shared.stopped.count_down();
    }
}

impl std::fmt::Debug for Driver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Driver")
            .field("finished", &self.finished())
            .finish()
    }
}
