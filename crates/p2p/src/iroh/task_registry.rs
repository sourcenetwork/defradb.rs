//! Every task the iroh endpoint spawns, aborted together at shutdown.

use std::future::Future;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use defra_core::thread_bounds::MaybeSend;
use kovan_queue::seg_queue::SegQueue;
use n0_future::task::JoinHandle;

use crate::tracked_task::{TrackedAbort, TrackedTask};

#[derive(Default)]
pub(super) struct TaskRegistry {
    closed: AtomicBool,
    registering: AtomicUsize,
    tasks: SegQueue<TrackedTask>,
}

impl TaskRegistry {
    /// Spawn into the registry, first dropping handles of finished tasks.
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
            self.reap_finished();
            let task = TrackedTask::spawn(future);
            let abort = task.abort_handle();
            self.tasks.push(task);
            Some(abort)
        };
        self.registering.fetch_sub(1, Ordering::SeqCst);
        abort
    }

    fn reap_finished(&self) {
        let mut live = Vec::new();
        while let Some(task) = self.tasks.pop() {
            if !task.is_finished() {
                live.push(task);
            }
        }
        for task in live {
            self.tasks.push(task);
        }
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
