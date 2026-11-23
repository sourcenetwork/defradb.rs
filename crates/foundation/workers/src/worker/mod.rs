//! A worker: one thread running a local executor (or, in driven mode, a
//! caller's thread driving one). Tasks are spawned onto it with
//! [`spawn_local`] and run there until they finish: no task stealing, and a
//! waker from any thread puts its task back on this worker's run queue.
//!
//! Each round a worker takes a batch from its inboxes, polls a batch of ready
//! tasks (each under `catch_unwind`, publishing a heartbeat), polls its hook
//! (the regolith `IoQueue`) between tasks, and fires due timers. With nothing
//! to do it spins briefly, then parks until a push, a wake, a timer or its
//! hook needs it. Everything here is touched only by the worker's own thread
//! except [`WorkerShared`], its face to other threads.

mod heartbeat;
mod keyed;
mod local;
mod run;
mod shared;
mod signal;
mod slab;
#[cfg(test)]
mod tests;
mod timers;
mod yield_now;

use std::any::Any;
use std::cell::RefCell;
use std::future::Future;
use std::panic::{self, AssertUnwindSafe};
use std::rc::Rc;
use std::time::Duration;

pub(crate) use heartbeat::REPORTED;
use local::Local;
pub(crate) use run::Worker;
pub use shared::WorkerStats;
pub(crate) use shared::{bump, Admitted, Job, WorkerShared};
pub(crate) use signal::{Parker, Signal};
pub use timers::{every, sleep, Sleep};
pub use yield_now::{yield_now, Budget, YieldNow};

use crate::completion::Completion;
use crate::{Error, WorkerId};

/// What the worker reads each round, fixed at start.
#[derive(Clone, Debug)]
pub(crate) struct Tuning {
    pub inbox_batch: usize,
    pub run_batch: usize,
    pub io_budget: usize,
    pub idle_spin: Duration,
    pub stuck_after: Duration,
    /// Another thread watches this worker's heartbeat. Without one (a single
    /// worker), the worker measures each poll itself when it returns.
    pub monitored: bool,
}

thread_local! {
    pub(super) static CURRENT: RefCell<Option<Rc<Local>>> = const { RefCell::new(None) };
}

fn current() -> Option<Rc<Local>> {
    CURRENT.with(|current| current.borrow().clone())
}

/// The timers pending on the calling worker.
#[cfg(test)]
pub(crate) fn pending_timers() -> usize {
    current().map_or(0, |local| local.timers.borrow().pending())
}

/// Whether the calling thread is a worker, or drives one.
pub fn on_worker() -> bool {
    CURRENT.with(|current| current.borrow().is_some())
}

/// The worker the calling thread is, if it is one.
pub fn current_worker() -> Option<WorkerId> {
    CURRENT.with(|current| current.borrow().as_ref().map(|local| local.shared.id))
}

/// Runs `future` on the calling worker until it finishes; it never moves to
/// another thread, so it need not be `Send`. Errors off a worker.
pub fn spawn_local<F>(future: F) -> Result<Completion<F::Output>, Error>
where
    F: Future + 'static,
    F::Output: 'static,
{
    let local = current().ok_or(Error::NotOnWorker)?;
    local.spawn(std::any::type_name::<F>(), future, false)
}

/// [`spawn_local`] under `name`, the task's name in stuck and panic reports.
pub(crate) fn spawn_named<F>(name: &'static str, future: F) -> Result<Completion<F::Output>, Error>
where
    F: Future + 'static,
    F::Output: 'static,
{
    let local = current().ok_or(Error::NotOnWorker)?;
    local.spawn(name, future, false)
}

/// [`spawn_named`] for a daemon: shutdown does not wait for it, and it is
/// dropped (its completion failing) when the worker stops.
pub(crate) fn spawn_daemon_named<F>(
    name: &'static str,
    future: F,
) -> Result<Completion<F::Output>, Error>
where
    F: Future + 'static,
    F::Output: 'static,
{
    let local = current().ok_or(Error::NotOnWorker)?;
    local.spawn(name, future, true)
}

/// [`spawn_local`] for background work (a periodic job, a sweep) that
/// shutdown does not wait for: it is dropped when the worker stops.
pub fn spawn_daemon<F>(future: F) -> Result<(), Error>
where
    F: Future<Output = ()> + 'static,
{
    let local = current().ok_or(Error::NotOnWorker)?;
    local
        .spawn(std::any::type_name::<F>(), future, true)
        .map(drop)
}

/// The text of a panic payload.
pub(crate) fn panic_message(payload: &(dyn Any + Send)) -> String {
    payload
        .downcast_ref::<&str>()
        .map(|s| (*s).to_owned())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "a panic with no message".to_owned())
}

/// Drops `value` without letting a panic in its destructor reach the worker.
pub(super) fn drop_quietly<T>(value: T) {
    let _ = panic::catch_unwind(AssertUnwindSafe(move || drop(value)));
}

/// A duration in whole microseconds, the pool clock's unit.
pub(crate) fn micros(duration: Duration) -> u64 {
    u64::try_from(duration.as_micros()).unwrap_or(u64::MAX)
}

/// Reports a poll that held its worker past the limit: the one log line for
/// it, wherever it was detected.
pub(crate) fn report_long_poll(worker: usize, task: &str, elapsed: u64, limit: Duration) {
    tracing::warn!(
        task,
        worker,
        elapsed_ms = elapsed / 1000,
        limit_ms = u64::try_from(limit.as_millis()).unwrap_or(u64::MAX),
        "a task held its worker past the limit; move blocking work to spawn_blocking"
    );
}
