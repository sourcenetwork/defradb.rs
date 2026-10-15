//! Every task the iroh endpoint spawns, aborted together at shutdown.

use std::future::Future;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use defra_core::thread_bounds::MaybeSend;
use kovan_queue::seg_queue::SegQueue;
use n0_future::task::JoinHandle;

use crate::tracked_task::{TrackedAbort, TrackedTask};

/// Tracked tasks below which a spawn does not reap.
pub(super) const REAP_FLOOR: usize = 64;

#[derive(Default)]
pub(super) struct TaskRegistry {
    closed: AtomicBool,
    registering: AtomicUsize,
    tasks: SegQueue<TrackedTask>,
    /// Queue length at which the next spawn reaps: twice the tasks the last
    /// reap kept, and never below [`REAP_FLOOR`]. A reap walks every tracked
    /// task, so reaping only after the queue doubles keeps a spawn O(1)
    /// amortized however many long-lived tasks the endpoint holds.
    reap_at: AtomicUsize,
    #[cfg(test)]
    reaps: AtomicUsize,
}

impl TaskRegistry {
    /// Spawn into the registry, first dropping handles of finished tasks once
    /// enough have accumulated.
    /// Returns `None` once closed, without polling the future.
    pub(super) fn spawn(
        &self,
        future: impl Future<Output = ()> + MaybeSend + 'static,
    ) -> Option<TrackedAbort> {
        // `close` raises the flag and then waits for this count to drain, so
        // a task is either refused here or pushed before the drain in `close`.
        self.registering.fetch_add(1, Ordering::SeqCst);
        let abort = if self.closed.load(Ordering::SeqCst) {
            None
        } else {
            if self.tasks.len() >= self.reap_at.load(Ordering::Relaxed).max(REAP_FLOOR) {
                self.reap_finished();
            }
            let task = TrackedTask::spawn(future);
            let abort = task.abort_handle();
            self.tasks.push(task);
            Some(abort)
        };
        self.registering.fetch_sub(1, Ordering::SeqCst);
        abort
    }

    fn reap_finished(&self) {
        #[cfg(test)]
        self.reaps.fetch_add(1, Ordering::Relaxed);
        let mut live = Vec::new();
        while let Some(task) = self.tasks.pop() {
            if !task.is_finished() {
                live.push(task);
            }
        }
        self.reap_at.store(live.len() * 2, Ordering::Relaxed);
        for task in live {
            self.tasks.push(task);
        }
    }

    #[cfg(test)]
    pub(super) fn reaps(&self) -> usize {
        self.reaps.load(Ordering::Relaxed)
    }

    #[cfg(test)]
    pub(super) fn len(&self) -> usize {
        self.tasks.len()
    }

    #[cfg(test)]
    pub(super) fn is_closed(&self) -> bool {
        self.closed.load(Ordering::SeqCst)
    }

    /// Refuse further spawns, abort every task and hand back their join handles.
    pub(super) fn close(&self) -> Vec<JoinHandle<()>> {
        self.closed.store(true, Ordering::SeqCst);
        while self.registering.load(Ordering::SeqCst) > 0 {
            std::hint::spin_loop();
        }
        let mut handles = Vec::new();
        while let Some(task) = self.tasks.pop() {
            task.abort();
            handles.push(task.into_join_handle());
        }
        handles
    }
}

impl Drop for TaskRegistry {
    fn drop(&mut self) {
        while let Some(task) = self.tasks.pop() {
            task.abort();
        }
    }
}
