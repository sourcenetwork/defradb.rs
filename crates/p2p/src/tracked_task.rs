//! Spawned tasks whose completion can be read from anywhere, including from
//! inside the task itself.
//!
//! In a browser, n0-future keeps a task's state mutably borrowed for as long
//! as the task is being polled, so asking a handle whether its task has
//! finished panics when the asker is that task. Registries that prune on
//! registration are asked exactly that: work spawned from inside tracked work
//! walks a list holding its own handle. Completion is instead recorded in a
//! flag the task sets when its future is dropped, which also covers panics
//! and aborts.

use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;

use defra_core::thread_bounds::MaybeSend;
use edge::{Edge, EdgeError};
use futures::future::select;
use regolith::sync::Event;

/// A spawned task and the flag that says it is done. Dropping it stops the
/// task; awaiting it gives how the task ended.
pub struct TrackedTask {
    finished: Arc<AtomicBool>,
    stop: Arc<Event>,
    handle: edge::Task<()>,
}

/// Abort access to a [`TrackedTask`] that does not own its join handle.
#[derive(Debug, Clone)]
pub struct TrackedAbort {
    finished: Arc<AtomicBool>,
    stop: Arc<Event>,
}

struct MarkFinished(Arc<AtomicBool>);

impl Drop for MarkFinished {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Relaxed);
    }
}

impl TrackedTask {
    pub fn spawn(edge: &Edge, future: impl Future<Output = ()> + MaybeSend + 'static) -> Self {
        let finished = Arc::new(AtomicBool::new(false));
        let marker = MarkFinished(Arc::clone(&finished));
        let stop = Arc::new(Event::new());
        let stopped = Arc::clone(&stop);
        let handle = edge
            .spawn(async move {
                let _marker = marker;
                let _ = select(std::pin::pin!(future), std::pin::pin!(stopped.wait())).await;
            })
            .within(Duration::MAX);
        Self {
            finished,
            stop,
            handle,
        }
    }

    pub fn is_finished(&self) -> bool {
        self.finished.load(Ordering::Relaxed)
    }

    pub fn abort(&self) {
        self.stop.set();
    }

    pub fn abort_handle(&self) -> TrackedAbort {
        TrackedAbort {
            finished: Arc::clone(&self.finished),
            stop: Arc::clone(&self.stop),
        }
    }
}

impl Future for TrackedTask {
    type Output = Result<(), EdgeError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        Pin::new(&mut self.handle).poll(cx)
    }
}

impl Drop for TrackedTask {
    fn drop(&mut self) {
        self.stop.set();
    }
}

impl TrackedAbort {
    pub fn is_finished(&self) -> bool {
        self.finished.load(Ordering::Relaxed)
    }

    pub fn abort(&self) {
        self.stop.set();
    }
}

/// Tasks aborted together when the set is dropped, like a `JoinSet`.
pub struct TrackedTaskSet {
    edge: Edge,
    tasks: Vec<TrackedTask>,
}

impl TrackedTaskSet {
    /// An empty set whose tasks run on `edge`.
    pub fn new(edge: Edge) -> Self {
        Self {
            edge,
            tasks: Vec::new(),
        }
    }

    /// Spawn into the set, first dropping handles of tasks that have finished
    /// so the set tracks live work rather than every task ever spawned.
    pub fn spawn(
        &mut self,
        future: impl Future<Output = ()> + MaybeSend + 'static,
    ) -> TrackedAbort {
        self.tasks.retain(|task| !task.is_finished());
        let task = TrackedTask::spawn(&self.edge, future);
        let abort = task.abort_handle();
        self.tasks.push(task);
        abort
    }

    pub fn len(&self) -> usize {
        self.tasks.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tasks.is_empty()
    }

    /// Abort every task and hand back their join handles to await.
    pub fn abort_all(&mut self) -> Vec<TrackedTask> {
        let tasks = std::mem::take(&mut self.tasks);
        for task in &tasks {
            task.abort();
        }
        tasks
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[nuclei::test(threads = 0)]
    async fn completion_is_visible_from_inside_later_tracked_work() {
        let (release, released) = futures::channel::oneshot::channel::<()>();
        let task = TrackedTask::spawn(&edge::testing::edge(), async move {
            let _ = released.await;
        });
        assert!(!task.is_finished());

        release.send(()).unwrap();
        let abort = task.abort_handle();
        task.await.unwrap();
        assert!(abort.is_finished());
    }

    #[nuclei::test(threads = 0)]
    async fn an_aborted_task_counts_as_finished() {
        let task = TrackedTask::spawn(&edge::testing::edge(), std::future::pending());
        let abort = task.abort_handle();
        abort.abort();

        task.await.unwrap();
        assert!(abort.is_finished());
    }

    #[nuclei::test(threads = 0)]
    async fn the_set_prunes_finished_tasks_on_spawn() {
        let mut set = TrackedTaskSet::new(edge::testing::edge());
        set.spawn(async {});
        crate::rt::yield_now().await;
        let first = set.tasks[0].abort_handle();
        while !first.is_finished() {
            crate::rt::yield_now().await;
        }

        set.spawn(std::future::pending());
        assert_eq!(set.len(), 1);
    }

    #[nuclei::test(threads = 0)]
    async fn dropping_the_set_aborts_its_tasks() {
        let mut set = TrackedTaskSet::new(edge::testing::edge());
        let abort = set.spawn(std::future::pending());
        drop(set);

        while !abort.is_finished() {
            crate::rt::yield_now().await;
        }
    }
}
