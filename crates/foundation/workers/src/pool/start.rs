//! Starting a threaded pool: one thread per planned seat, each placing
//! itself before it builds anything, and a start-up that waits for all of
//! them and reports what the OS granted.

use std::sync::Arc;

use regolith::sync::{Latch, OnceCell};

use super::blocking::Blocking;
use super::monitor;
use super::placer::{Placer, ThreadPlacement};
use super::report::{Report, TopologySummary, WorkerReport};
use super::{build_hook, Pool, PoolShared};
use crate::config::Config;
use crate::hook::HookFactory;
use crate::ledger::Ledger;
use crate::topology::Topology;
use crate::worker::{every, Parker, Tuning, Worker};
use crate::{clock, os, placement, Error};

/// What each worker thread reports back while the pool starts.
struct Startup {
    placed: Box<[OnceCell<Result<ThreadPlacement, Error>>]>,
    ready: Latch,
}

impl Startup {
    fn report(&self, index: usize, placed: Result<ThreadPlacement, Error>) {
        let _ = self.placed[index].set(placed);
        self.ready.count_down();
    }
}

impl Pool {
    /// Starts a pool on this machine as `config` says. A placed pool runs on
    /// CPUs no other placed pool of the process holds, and holds them until
    /// it and everything placed through it are gone; when too few are free
    /// it shares, and its [`report`](Pool::report) says so. No OS setting it
    /// asks for is a hard error: each falls back to a weaker one, and the
    /// report says what was granted. Errors only on a configuration no pool
    /// can run, a thread the OS will not start, a hook factory that panics,
    /// or a call from a worker thread.
    ///
    /// ```no_run
    /// // A pool of its own, on the cores no other pool holds.
    /// let compute = workers::Pool::start(workers::Config::named("compute"))?;
    /// let answer = compute.submit_and_wait(workers::Target::LeastLoaded, || async { 6 * 7 })?;
    /// assert_eq!(answer, 42);
    /// # Ok::<(), workers::Error>(())
    /// ```
    pub fn start(config: Config) -> Result<Pool, Error> {
        Pool::start_in(config, Ledger::process())
    }

    /// [`start`](Pool::start), holding its CPUs in `ledger`.
    pub(crate) fn start_in(config: Config, ledger: Ledger<'static>) -> Result<Pool, Error> {
        config.validate()?;
        if crate::worker::on_worker() {
            return Err(Error::OnWorker);
        }
        let os = os::system();
        let allowed = os.thread_affinity().unwrap_or_default();
        let topology = Topology::detect();
        let placement::Reserved { plan, hold } = placement::reserve(&topology, &config, ledger);
        let notes = plan.notes.clone();
        let memory_lock = os::lock_memory(os, &config.memory_lock);
        let clock = config
            .clock
            .clone()
            .or_else(clock::platform)
            .ok_or_else(|| Error::Config("this target has no clock; set Config::clock".into()))?;
        let placer = Arc::new(
            Placer::new(
                plan,
                allowed,
                config.priorities.clone(),
                config.memory_policy.clone(),
            )
            .holding(hold),
        );
        let blocking = Blocking::start(
            config.blocking_threads,
            config.blocking_queue,
            &config.thread_name,
            placer.clone(),
        )?;
        let parkers = placer
            .plan
            .seats
            .iter()
            .map(|_| Parker::Thread(OnceCell::new()))
            .collect();
        let mut shared = PoolShared::new(&config, &topology, placer, clock, parkers);
        shared.blocking = Some(blocking);
        let shared = Arc::new(shared);
        let count = shared.workers.len();
        let startup = Arc::new(Startup {
            placed: (0..count).map(|_| OnceCell::new()).collect(),
            ready: Latch::new(count),
        });
        let driven_monitor = config.monitor.clone();
        let tuning = PoolShared::tuning(&config, count > 1 || driven_monitor.is_some());
        let watchers = if driven_monitor.is_some() {
            0
        } else {
            monitor::WATCHERS
        };
        let mut threads = Vec::with_capacity(count);
        for index in 0..count {
            let thread = {
                let (shared, startup, tuning, hook) = (
                    shared.clone(),
                    startup.clone(),
                    tuning.clone(),
                    config.hook.clone(),
                );
                let recheck = config.recheck_every;
                std::thread::Builder::new()
                    .name(format!("{}-{index}", config.thread_name))
                    .spawn(move || {
                        let watches = index < watchers;
                        run_worker(&shared, index, tuning, hook, recheck, watches, &startup)
                    })
            };
            match thread {
                Ok(thread) => threads.push(thread),
                Err(e) => {
                    for missing in index..count {
                        startup.report(missing, Err(Error::Spawn(e.to_string())));
                        shared.stopped.count_down();
                    }
                    break;
                }
            }
        }
        crate::completion::block_on(startup.ready.wait())?;
        let mut workers = Vec::with_capacity(count);
        for (index, placed) in startup.placed.iter().enumerate() {
            match placed.get() {
                Some(Ok(placement)) => workers.push(WorkerReport {
                    id: shared.workers[index].id,
                    placement: placement.clone(),
                }),
                Some(Err(error)) => {
                    let error = error.clone();
                    shared.shutdown();
                    shared.abandon();
                    return Err(error);
                }
                None => {
                    shared.shutdown();
                    shared.abandon();
                    return Err(Error::Spawn(format!("worker {index} did not start")));
                }
            }
        }
        let report = Report {
            topology: TopologySummary::of(&topology),
            workers,
            memory_lock,
            notes,
        };
        if let Some(driver) = driven_monitor {
            driver(monitor::Monitor::of(&shared));
        }
        Ok(Pool::from_parts(shared, report, &config, threads))
    }
}

/// Counts a worker out of the pool's stop latch however its thread ends.
struct CountOut<'a>(&'a Latch);

impl Drop for CountOut<'_> {
    fn drop(&mut self) {
        self.0.count_down();
    }
}

/// Reports a worker's start once; a thread that ends before it reported
/// (a panic while starting) reports the failure instead of hanging start.
struct ReportIn<'a> {
    startup: &'a Startup,
    index: usize,
    sent: bool,
}

impl ReportIn<'_> {
    fn send(&mut self, placed: Result<ThreadPlacement, Error>) {
        if !std::mem::replace(&mut self.sent, true) {
            self.startup.report(self.index, placed);
        }
    }
}

impl Drop for ReportIn<'_> {
    fn drop(&mut self) {
        let index = self.index;
        self.send(Err(Error::Spawn(format!(
            "worker {index} stopped while starting"
        ))));
    }
}

/// A worker thread's whole life: place itself, build its hook, start the
/// pool's own periodic jobs, report in, run, and count itself out.
fn run_worker(
    shared: &Arc<PoolShared>,
    index: usize,
    tuning: Tuning,
    hook: Option<HookFactory>,
    recheck: std::time::Duration,
    watches: bool,
    startup: &Startup,
) {
    let _out = CountOut(&shared.stopped);
    let mut report_in = ReportIn {
        startup,
        index,
        sent: false,
    };
    let worker_shared = shared.workers[index].clone();
    worker_shared.signal.bind_current_thread();
    let placed = shared.placer.place_worker(index);
    let id = worker_shared.id;
    let hook = match build_hook(hook.as_ref(), id) {
        Ok(hook) => hook,
        Err(error) => return report_in.send(Err(error)),
    };
    let worker = match Worker::new(worker_shared.clone(), shared.clock.clone(), tuning, hook) {
        Ok(worker) => worker,
        Err(error) => return report_in.send(Err(error)),
    };
    if watches && shared.workers.len() > 1 {
        let pool = shared.clone();
        worker.spawn_daemon("workers::monitor", async move {
            let period = monitor::period(pool.stuck_after);
            let _ = every(period, || {
                monitor::scan(&pool, pool.clock.now_micros());
                async {}
            })
            .await;
        });
    }
    let report = placed.report.clone();
    {
        let pool = shared.clone();
        let mine = worker_shared.clone();
        let mut warned = [false; 2];
        worker.spawn_daemon("workers::recheck", async move {
            let _ = every(recheck, || {
                pool.placer.recheck(&placed, &mine, &mut warned);
                async {}
            })
            .await;
        });
    }
    report_in.send(Ok(report));
    worker.run();
}
