//! The per-worker hook: work a worker does between tasks that the pool itself
//! knows nothing about. For regolith it is the worker's `IoQueue`: polled
//! between tasks so reads, commit fsyncs and stalls owed to this thread
//! complete here, and handed the worker's waker before it parks so a
//! completion pushed from another thread wakes it.

use std::sync::Arc;
use std::task::Waker;
use std::time::Duration;

use crate::WorkerId;

/// What one [`Hook::poll`] did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HookProgress {
    /// Completions delivered: tasks they woke are now runnable.
    pub completed: usize,
    /// The hook still owes work.
    pub more: bool,
    /// The hook must be polled again within this long even if nothing wakes
    /// the worker.
    pub wake_in: Option<Duration>,
}

/// Work a worker runs between task polls. Built on the worker's own thread,
/// so it need not be `Send`.
pub trait Hook {
    /// Run at most `budget` units of owed work and take in completions other
    /// threads pushed. Never waits.
    fn poll(&mut self, budget: usize) -> HookProgress;

    /// The worker is about to park: wake `waker` (once) when the hook has
    /// work, or at once if it has work now.
    fn idle(&mut self, waker: &Waker);
}

/// Builds one worker's [`Hook`], on that worker's thread, after it is pinned
/// and its memory policy is set, so what the hook allocates is node-local.
///
/// ```no_run
/// use std::sync::Arc;
///
/// let db = Arc::new(regolith::Db::open("data", regolith::Options::default()).unwrap());
/// let hook: workers::HookFactory = Arc::new(move |_worker| Box::new(db.io_queue()));
/// let config = workers::Config { hook: Some(hook), ..workers::Config::default() };
/// # let _ = config;
/// ```
pub type HookFactory = Arc<dyn Fn(WorkerId) -> Box<dyn Hook> + Send + Sync>;

impl Hook for regolith::IoQueue {
    fn poll(&mut self, budget: usize) -> HookProgress {
        let progress = regolith::IoQueue::poll(self, regolith::IoBudget::units(budget));
        HookProgress {
            completed: progress.completed,
            more: progress.more_pending,
            wake_in: progress.wake_in,
        }
    }

    fn idle(&mut self, waker: &Waker) {
        self.idle_waker(waker);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_io_queue_with_nothing_owed_reports_nothing() {
        let db = regolith::Db::open("hook", regolith::Options::memory()).unwrap();
        let mut queue: Box<dyn Hook> = Box::new(db.io_queue());
        assert_eq!(queue.poll(8), HookProgress::default());
    }
}
