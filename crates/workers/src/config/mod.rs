//! Everything the pool can be told, in one [`Config`], resolved once at start.
//!
//! No setting here is ever a hard error: a placement, priority, memory policy
//! or memory lock the OS refuses falls back to the next weaker one, with one
//! warning, and the start-up report says what was asked and what was granted.

use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use crate::clock::Clock;
use crate::hook::HookFactory;
use crate::pool::MonitorDriver;
use crate::topology::CpuSet;

mod parse;
mod priority;

pub use priority::{Priorities, Priority, QosClass, RealTimePolicy, ThreadClass};

/// How many workers to start.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Workers {
    /// Linux: the allowed physical cores (logical CPUs with [`Smt::All`]),
    /// within the cgroup CPU quota, minus the edge's and compaction's threads.
    /// macOS: the performance cores. Elsewhere: the available parallelism.
    /// A placed pool counts only the cores no other placed pool holds, and
    /// what those hold counts against the quota; with none free it is one
    /// unpinned worker, and its report says so.
    Auto,
    /// Exactly this many (at least one). A placed pool that cannot give each
    /// a CPU no other pool holds shares CPUs with them, and its report says
    /// so.
    Count(usize),
}

/// Where workers run. A pinned (placed) pool holds whole cores no other
/// placed pool of the process holds: its workers' cores and the spare cores
/// its other threads run on, until it and everything placed through it are
/// gone. Where the OS cannot pin (macOS), the hold still divides the
/// machine's cores between pools for sizing, and the OS decides where each
/// thread runs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Placement {
    /// Not pinned: the OS places them, beside whatever runs, and the pool
    /// holds no CPU. The default on macOS, which cannot pin.
    Unpinned,
    /// Pinned, alternating NUMA nodes, and L3 domains within a node, so load
    /// reaches every memory controller and cache first. The default on Linux.
    Spread,
    /// Pinned, filling one node (and one L3 domain) before the next.
    Pack,
    /// Pinned to these sets, one per worker; the worker count is their number.
    Custom(Vec<CpuSet>),
}

/// Whether workers use SMT siblings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Smt {
    /// One worker per physical core, on its first sibling. Siblings are used
    /// only when more workers than cores are asked for.
    PhysicalOnly,
    /// One worker per logical CPU, every core's first sibling before any
    /// second.
    All,
}

/// The NUMA memory policy each worker sets for itself after pinning.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MemoryPolicy {
    /// Leave the process's policy (first touch).
    Off,
    /// Allocate on the node the worker runs on (`MPOL_LOCAL`).
    Local,
    /// Allocate only on these nodes (`MPOL_BIND`); falls back to local.
    Bind(Vec<usize>),
}

/// Locking the whole process's memory so nothing it uses is paged out.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MemoryLock {
    /// No locking.
    Off,
    /// `mlockall`: always what is mapped now, and with `future` what is
    /// mapped later. `on_fault` locks pages as they are touched instead of
    /// populating every mapping at once (Linux 4.4+). Locking future mappings
    /// makes an allocation fail once locked bytes pass `RLIMIT_MEMLOCK`, so
    /// it is attempted only when the limit is unlimited or at least
    /// `footprint` bytes; otherwise the lock falls back to current mappings.
    Lock {
        /// Lock mappings made after start too (`MCL_FUTURE`).
        future: bool,
        /// Lock pages when first touched (`MCL_ONFAULT`).
        on_fault: bool,
        /// The bytes the process expects to keep locked: block cache, write
        /// buffers, worker state, lens memory.
        footprint: Option<u64>,
    },
}

/// The pool's whole configuration. `Config::default()` is the recommended
/// production setting; change fields as needed.
///
/// ```
/// let config = workers::Config {
///     workers: workers::Workers::Count(4),
///     ..workers::Config::default()
/// };
/// assert_eq!(config.workers, workers::Workers::Count(4));
/// ```
#[derive(Clone)]
pub struct Config {
    /// How many workers.
    pub workers: Workers,
    /// Where they run.
    pub placement: Placement,
    /// Whether they use SMT siblings.
    pub smt: Smt,
    /// Each worker's NUMA memory policy.
    pub memory_policy: MemoryPolicy,
    /// Locking the process's memory, done once at start.
    pub memory_lock: MemoryLock,
    /// Each thread class's priority.
    pub priorities: Priorities,
    /// Cores kept for the network edge when sizing [`Workers::Auto`];
    /// `None` is none while a network runtime holds cores of its own
    /// ([`EdgeCores`](crate::EdgeCores)), else one per node, at least two.
    pub edge_threads: Option<usize>,
    /// Cores kept for regolith's compaction threads when sizing
    /// [`Workers::Auto`].
    pub compaction_threads: usize,
    /// Submissions a worker admits at once (queued plus running).
    pub inbox_items: usize,
    /// Bytes of submissions a worker admits at once.
    pub inbox_bytes: usize,
    /// Inbox messages a worker takes per round.
    pub inbox_batch: usize,
    /// Task polls a worker runs per round before it looks at its inbox again.
    pub run_batch: usize,
    /// Units of I/O the per-worker hook may run per poll between tasks.
    pub io_budget: usize,
    /// How long an idle worker spins before it parks.
    pub idle_spin: Duration,
    /// A poll running longer than this is reported and its worker is routed
    /// around until it returns.
    pub stuck_after: Duration,
    /// How often each worker re-reads its pinning and priority to catch an
    /// external reset.
    pub recheck_every: Duration,
    /// Threads in the blocking pool.
    pub blocking_threads: usize,
    /// Blocking jobs queued at most before `spawn_blocking` waits.
    pub blocking_queue: usize,
    /// How long [`Pool::shutdown_blocking`](crate::Pool::shutdown_blocking)
    /// waits for the workers to drain.
    pub shutdown_deadline: Duration,
    /// Worker thread names are this plus the worker's index.
    pub thread_name: String,
    /// The clock timers and the stuck-poll monitor read. `None` is the
    /// platform's monotonic clock; required where there is none (wasm).
    pub clock: Option<Arc<dyn Clock>>,
    /// Builds each worker's hook on the worker's own thread, polled between
    /// tasks (for regolith: the worker's `IoQueue`).
    pub hook: Option<HookFactory>,
    /// Runs the stuck-poll scan on a timer outside the pool (the network
    /// edge's); `None` has two workers run it on each other.
    pub monitor: Option<MonitorDriver>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            workers: Workers::Auto,
            placement: if cfg!(target_os = "linux") {
                Placement::Spread
            } else {
                Placement::Unpinned
            },
            smt: Smt::PhysicalOnly,
            memory_policy: MemoryPolicy::Local,
            memory_lock: MemoryLock::Off,
            priorities: Priorities::default(),
            edge_threads: None,
            compaction_threads: regolith::DEFAULT_MAX_BACKGROUND_COMPACTIONS,
            inbox_items: 1024,
            inbox_bytes: 64 << 20,
            inbox_batch: 64,
            run_batch: 64,
            io_budget: 32,
            idle_spin: Duration::from_micros(50),
            stuck_after: Duration::from_millis(10),
            recheck_every: Duration::from_secs(1),
            blocking_threads: 2,
            blocking_queue: 256,
            shutdown_deadline: Duration::from_secs(30),
            thread_name: "defra-worker".to_owned(),
            clock: None,
            hook: None,
            monitor: None,
        }
    }
}

impl Config {
    /// A pool of its own for one kind of work (compute, say): as many
    /// workers as there are cores no other pool of the process holds
    /// ([`Workers::Auto`]), placed as by default, keeping no cores for a
    /// network edge or compaction, its threads named `name-<index>`. Change
    /// fields as needed; the pool finds its CPUs, never its caller.
    ///
    /// ```no_run
    /// let compute = workers::Pool::start(workers::Config::named("compute"))?;
    /// let sum = compute.submit_and_wait(workers::Target::LeastLoaded, || async {
    ///     (1..=10u64).sum::<u64>()
    /// })?;
    /// assert_eq!(sum, 55);
    /// # Ok::<(), workers::Error>(())
    /// ```
    pub fn named(name: &str) -> Config {
        Config {
            thread_name: name.to_owned(),
            edge_threads: Some(0),
            compaction_threads: 0,
            ..Config::default()
        }
    }

    /// `workers` workers that leave the machine as they found it: unpinned,
    /// at the priority and memory policy their threads start with, beside
    /// one blocking thread. For a pool that runs beside other work rather
    /// than owning the machine: a test, a one-shot tool.
    pub fn unplaced(workers: usize) -> Config {
        Config {
            workers: Workers::Count(workers.max(1)),
            placement: Placement::Unpinned,
            memory_policy: MemoryPolicy::Off,
            priorities: Priorities::inherited(),
            blocking_threads: 1,
            ..Config::default()
        }
    }

    /// Rejects values no pool can run with.
    pub(crate) fn validate(&self) -> Result<(), crate::Error> {
        let positive = [
            ("inbox_items", self.inbox_items),
            ("inbox_bytes", self.inbox_bytes),
            ("inbox_batch", self.inbox_batch),
            ("run_batch", self.run_batch),
            ("blocking_threads", self.blocking_threads),
            ("blocking_queue", self.blocking_queue),
        ];
        if let Some((name, _)) = positive.iter().find(|(_, value)| *value == 0) {
            return Err(crate::Error::Config(format!("{name} must be at least 1")));
        }
        if self.workers == Workers::Count(0) {
            return Err(crate::Error::Config("workers must be at least 1".into()));
        }
        if let Placement::Custom(sets) = &self.placement {
            if sets.is_empty() || sets.iter().any(CpuSet::is_empty) {
                return Err(crate::Error::Config(
                    "every custom CPU set must name a CPU".into(),
                ));
            }
            match self.workers {
                Workers::Count(n) if n != sets.len() => {
                    return Err(crate::Error::Config(format!(
                        "{n} workers asked for but {} custom CPU sets given",
                        sets.len()
                    )));
                }
                _ => {}
            }
        }
        if self.stuck_after.is_zero() || self.recheck_every.is_zero() {
            return Err(crate::Error::Config(
                "stuck_after and recheck_every must be above zero".into(),
            ));
        }
        Ok(())
    }
}

impl fmt::Debug for Config {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Config")
            .field("workers", &self.workers)
            .field("placement", &self.placement)
            .field("smt", &self.smt)
            .field("memory_policy", &self.memory_policy)
            .field("memory_lock", &self.memory_lock)
            .field("priorities", &self.priorities)
            .field("edge_threads", &self.edge_threads)
            .field("compaction_threads", &self.compaction_threads)
            .field("inbox_items", &self.inbox_items)
            .field("inbox_bytes", &self.inbox_bytes)
            .field("inbox_batch", &self.inbox_batch)
            .field("run_batch", &self.run_batch)
            .field("io_budget", &self.io_budget)
            .field("idle_spin", &self.idle_spin)
            .field("stuck_after", &self.stuck_after)
            .field("recheck_every", &self.recheck_every)
            .field("blocking_threads", &self.blocking_threads)
            .field("blocking_queue", &self.blocking_queue)
            .field("shutdown_deadline", &self.shutdown_deadline)
            .field("thread_name", &self.thread_name)
            .field("clock", &self.clock)
            .field("hook", &self.hook.as_ref().map(|_| "set"))
            .field("monitor", &self.monitor.as_ref().map(|_| "set"))
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_is_valid_and_workers_rank_first() {
        let config = Config::default();
        config.validate().unwrap();
        #[cfg(target_os = "linux")]
        assert_eq!(
            config.priorities.of(ThreadClass::Worker),
            Priority::RealTime {
                policy: RealTimePolicy::RoundRobin,
                priority: 50
            }
        );
        #[cfg(target_os = "macos")]
        assert_eq!(
            config.priorities.of(ThreadClass::Worker),
            Priority::Qos(QosClass::UserInteractive)
        );
    }

    #[test]
    fn rejects_zero_bounds_and_mismatched_custom_sets() {
        let zero = Config {
            inbox_items: 0,
            ..Config::default()
        };
        assert!(zero.validate().is_err());
        let none = Config {
            workers: Workers::Count(0),
            ..Config::default()
        };
        assert!(none.validate().is_err());
        let mismatched = Config {
            workers: Workers::Count(3),
            placement: Placement::Custom(vec![CpuSet::parse("0").unwrap()]),
            ..Config::default()
        };
        assert!(mismatched.validate().is_err());
        let empty_set = Config {
            placement: Placement::Custom(vec![CpuSet::new()]),
            ..Config::default()
        };
        assert!(empty_set.validate().is_err());
    }
}
