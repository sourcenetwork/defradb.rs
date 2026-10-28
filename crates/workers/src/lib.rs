//! A thread-per-core worker pool, and everything about threads: the machine's
//! topology, where workers run, their priority and memory policy, each
//! worker's local executor, routing work to a worker, admission, blocking
//! code kept off the workers, and shutdown.
//!
//! On wasm, which has no threads, only the one-worker driven mode
//! ([`Pool::driven`]) runs; the threaded pool's parts still compile there,
//! unused.
#![cfg_attr(target_family = "wasm", allow(dead_code))]

mod clock;
mod completion;
mod config;
mod error;
mod hook;
#[cfg(not(target_family = "wasm"))]
mod ledger;
mod os;
mod placement;
mod pool;
pub mod topology;
mod worker;

#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
pub use clock::StdClock;
pub use clock::{Clock, EnvClock};
#[cfg(not(target_family = "wasm"))]
pub use completion::{block_on, block_on_timeout};
pub use completion::{CancelOnDrop, Completion};
pub use config::{
    Config, MemoryLock, MemoryPolicy, Placement, Priorities, Priority, QosClass, RealTimePolicy,
    Smt, ThreadClass, Workers,
};
pub use error::Error;
pub use hook::{Hook, HookFactory, HookProgress};
pub use os::{current_affinity, ContextSwitches, Outcome};
#[cfg(not(target_family = "wasm"))]
pub use placement::{edge_cores, EdgeCores};
pub use pool::{
    default_pool, delay, delay_on, run_on, spawn, spawn_background, spawn_detached, timeout,
    timeout_on, Background, Delay, Driver, Monitor, MonitorDriver, PlacedEnv, Pool, Report, Stats,
    Submit, Target, ThreadPlacement, TopologySummary, WorkerReport,
};
pub use worker::{
    current_worker, every, on_worker, sleep, spawn_daemon, spawn_local, yield_now, Budget, Sleep,
    WorkerStats, YieldNow,
};

/// A worker's identity: its index in the pool and the NUMA node it runs on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct WorkerId {
    /// Its index, from 0.
    pub index: usize,
    /// The NUMA node it is placed on (0 when unpinned or without NUMA).
    pub node: usize,
}
