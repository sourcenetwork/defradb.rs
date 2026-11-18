//! The process's default pool: where work submitted without a pool runs (a
//! spawn or a timer off every worker, a database's work when it was given no
//! pool).
//!
//! It is the pool registered with [`Pool::make_default`] while that one runs
//! (a node's data-path pool registers itself), and otherwise a pool started
//! on first use and kept for the life of the process: [`Workers::Auto`] on
//! the cores no other pool holds, at the priority the process started with.
//! A registered pool that is shutting down is passed over, so the deadline
//! of its own [`Pool::stop`] never waits on itself. In a browser no thread
//! can be started: the default is the registered pool or none.

use std::sync::{Arc, Weak};

use regolith::sync::{Atom, Lazy, OnceCell};

use super::{Handle, Pool};
#[cfg(not(target_family = "wasm"))]
use crate::config::{Config, Priorities};
use crate::Error;

/// The registered pool, and the one started on first use without one.
pub(crate) struct DefaultPool {
    registered: Atom<Option<Weak<Handle>>>,
    started: OnceCell<Pool>,
}

static PROCESS: Lazy<DefaultPool> = Lazy::new(DefaultPool::new);

/// The process's default pool: the registered one while it runs, else the
/// one started on first use. Errors when none runs and none can start: on a
/// worker thread before it started (a start waits for its threads), in a
/// browser, or when the OS will not start its threads.
///
/// ```no_run
/// let pool = workers::default_pool()?;
/// let answer = pool.submit_and_wait(workers::Target::LeastLoaded, || async { 6 * 7 })?;
/// assert_eq!(answer, 42);
/// # Ok::<(), workers::Error>(())
/// ```
pub fn default_pool() -> Result<Pool, Error> {
    PROCESS.get(start_default)
}

#[cfg(not(target_family = "wasm"))]
fn start_default() -> Result<Pool, Error> {
    Pool::start(default_config())
}

/// A browser starts no threads: its one worker is the page's own.
#[cfg(target_family = "wasm")]
fn start_default() -> Result<Pool, Error> {
    Err(Error::NotOnWorker)
}

/// The pool started when none is registered.
#[cfg(not(target_family = "wasm"))]
fn default_config() -> Config {
    Config {
        priorities: Priorities::inherited(),
        ..Config::named("defra-default")
    }
}

impl Pool {
    /// Makes this pool the process's default pool ([`default_pool`]) until
    /// it shuts down, unless another registered pool still runs. Returns
    /// whether this pool is the default now. The registration does not keep
    /// the pool alive.
    pub fn make_default(&self) -> bool {
        PROCESS.register(self)
    }
}

impl DefaultPool {
    fn new() -> DefaultPool {
        DefaultPool {
            registered: Atom::new(None),
            started: OnceCell::new(),
        }
    }

    /// The registered pool, if it is still running.
    fn registered(&self) -> Option<Pool> {
        running(&self.registered.load())
    }

    fn register(&self, pool: &Pool) -> bool {
        loop {
            let current = self.registered.load();
            if let Some(running) = running(&current) {
                return Arc::ptr_eq(&running.handle, &pool.handle);
            }
            let next = Some(Arc::downgrade(&pool.handle));
            if self.registered.compare_and_swap(&current, next).is_ok() {
                return true;
            }
        }
    }

    pub(crate) fn get<S>(&self, start: S) -> Result<Pool, Error>
    where
        S: FnOnce() -> Result<Pool, Error>,
    {
        if let Some(pool) = self.registered() {
            return Ok(pool);
        }
        self.started(start)
    }

    /// The pool started on first use: one start at a time, the rest wait
    /// for it; a start that fails leaves the next caller to try again.
    #[cfg(not(target_family = "wasm"))]
    fn started<S>(&self, start: S) -> Result<Pool, Error>
    where
        S: FnOnce() -> Result<Pool, Error>,
    {
        if let Some(pool) = self.started.get() {
            return Ok(pool.clone());
        }
        let started = crate::completion::block_on(self.started.get_or_try_init(async { start() }))?;
        started.cloned()
    }

    /// A browser has no thread to wait on for a start.
    #[cfg(target_family = "wasm")]
    fn started<S>(&self, start: S) -> Result<Pool, Error>
    where
        S: FnOnce() -> Result<Pool, Error>,
    {
        self.started.get().cloned().map_or_else(start, Ok)
    }
}

/// The pool `registered` names, if it is still running.
fn running(registered: &Option<Weak<Handle>>) -> Option<Pool> {
    let pool = Pool {
        handle: registered.as_ref()?.upgrade()?,
    };
    (!pool.is_closing()).then_some(pool)
}

#[cfg(all(test, not(target_family = "wasm")))]
mod tests {
    use std::time::{Duration, Instant};

    use super::*;
    use crate::config::{Placement, Workers};
    use crate::ledger::tests::ledger;
    use crate::Target;

    fn same(a: &Pool, b: &Pool) -> bool {
        Arc::ptr_eq(&a.handle, &b.handle)
    }

    #[test]
    fn the_default_is_auto_on_free_cores_at_the_inherited_priority() {
        let config = default_config();
        assert_eq!(config.workers, Workers::Auto);
        assert_eq!(config.placement, Config::default().placement);
        assert_eq!(config.priorities, Priorities::inherited());
        assert_eq!(
            (config.edge_threads, config.compaction_threads),
            (Some(0), 0)
        );
    }

    #[test]
    fn the_default_is_the_registered_pool_while_it_runs_else_a_placed_pool_started_once() {
        let slot = DefaultPool::new();
        let ledger = ledger();
        let started = || {
            Pool::start_in(
                Config {
                    workers: Workers::Count(2),
                    ..default_config()
                },
                ledger,
            )
        };
        let lazy = slot.get(started).unwrap();
        #[cfg(target_os = "linux")]
        {
            let pinned = lazy
                .report()
                .workers
                .iter()
                .all(|worker| worker.placement.pinning.granted.starts_with("CPUs"));
            assert!(pinned, "{}", lazy.report());
            let allowed = crate::os::current_affinity().unwrap();
            assert!(ledger.free(&allowed).len() < allowed.len(), "it holds CPUs");
        }
        assert!(
            same(&slot.get(|| unreachable!("started once")).unwrap(), &lazy),
            "started once"
        );

        let node = Pool::start(Config::unplaced(1)).unwrap();
        assert!(slot.register(&node));
        let other = Pool::start(Config::unplaced(1)).unwrap();
        assert!(
            !slot.register(&other),
            "the first registered pool still runs"
        );
        assert!(same(&slot.get(|| unreachable!()).unwrap(), &node));
        assert_eq!(
            slot.get(|| unreachable!())
                .unwrap()
                .submit_and_wait(Target::LeastLoaded, || async { 7 }),
            Ok(7)
        );

        node.shutdown();
        assert!(
            same(&slot.get(|| unreachable!()).unwrap(), &lazy),
            "a registered pool that shuts down is passed over"
        );
        assert!(slot.register(&other), "and can be replaced");
        drop(other);
        assert!(same(&slot.get(|| unreachable!()).unwrap(), &lazy));
        node.shutdown_blocking().unwrap();
        lazy.shutdown_blocking().unwrap();
    }

    #[test]
    fn a_stopped_placed_pools_cpus_go_back_to_the_ledger() {
        let ledger = ledger();
        let allowed = crate::os::current_affinity().unwrap_or_default();
        let pool = Pool::start_in(
            Config {
                workers: Workers::Count(1),
                placement: Placement::Spread,
                ..Config::unplaced(1)
            },
            ledger,
        )
        .unwrap();
        #[cfg(target_os = "linux")]
        assert!(ledger.free(&allowed).len() < allowed.len(), "it holds CPUs");
        pool.shutdown_blocking().unwrap();
        drop(pool);
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut pause = Duration::from_micros(100);
        while ledger.free(&allowed) != allowed {
            assert!(Instant::now() < deadline, "the CPUs were never given back");
            std::thread::park_timeout(pause);
            pause = (pause * 2).min(Duration::from_millis(20));
        }
    }
}
