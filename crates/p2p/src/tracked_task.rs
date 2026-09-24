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
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use defra_core::thread_bounds::MaybeSend;
use n0_future::task::{AbortHandle, JoinHandle};

/// A spawned task and the flag that says it is done.
#[derive(Debug)]
pub struct TrackedTask {
    finished: Arc<AtomicBool>,
    handle: JoinHandle<()>,
}

/// Abort access to a [`TrackedTask`] that does not own its join handle.
#[derive(Debug, Clone)]
pub struct TrackedAbort {
    finished: Arc<AtomicBool>,
    abort: AbortHandle,
}

struct MarkFinished(Arc<AtomicBool>);

impl Drop for MarkFinished {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Relaxed);
    }
}

impl TrackedTask {
    pub fn spawn(future: impl Future<Output = ()> + MaybeSend + 'static) -> Self {
        let finished = Arc::new(AtomicBool::new(false));
        let marker = MarkFinished(Arc::clone(&finished));
        let handle = n0_future::task::spawn(async move {
            let _marker = marker;
            future.await;
        });
        Self { finished, handle }
    }

    pub fn is_finished(&self) -> bool {
        self.finished.load(Ordering::Relaxed)
    }

    pub fn abort(&self) {
        self.handle.abort();
    }

    pub fn abort_handle(&self) -> TrackedAbort {
        TrackedAbort {
            finished: Arc::clone(&self.finished),
            abort: self.handle.abort_handle(),
        }
    }

    pub fn into_join_handle(self) -> JoinHandle<()> {
        self.handle
    }
}

impl TrackedAbort {
    pub fn is_finished(&self) -> bool {
        self.finished.load(Ordering::Relaxed)
    }

    pub fn abort(&self) {
        self.abort.abort();
    }
}

/// Tasks aborted together when the set is dropped, like a `JoinSet`.
#[derive(Debug, Default)]
pub struct TrackedTaskSet {
    tasks: Vec<TrackedTask>,
}

impl TrackedTaskSet {
    /// Spawn into the set, first dropping handles of tasks that have finished
    /// so the set tracks live work rather than every task ever spawned.
    pub fn spawn(
        &mut self,
        future: impl Future<Output = ()> + MaybeSend + 'static,
    ) -> TrackedAbort {
        self.tasks.retain(|task| !task.is_finished());
        let task = TrackedTask::spawn(future);
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
    pub fn abort_all(&mut self) -> Vec<JoinHandle<()>> {
        std::mem::take(&mut self.tasks)
            .into_iter()
            .map(|task| {
                task.abort();
                task.into_join_handle()
            })
            .collect()
    }
}

impl Drop for TrackedTaskSet {
    fn drop(&mut self) {
        for task in &self.tasks {
            task.abort();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn completion_is_visible_from_inside_later_tracked_work() {
        let (release, released) = tokio::sync::oneshot::channel::<()>();
        let task = TrackedTask::spawn(async move {
            let _ = released.await;
        });
        assert!(!task.is_finished());

        release.send(()).unwrap();
        let abort = task.abort_handle();
        task.into_join_handle().await.unwrap();
        assert!(abort.is_finished());
    }

    #[tokio::test]
    async fn an_aborted_task_counts_as_finished() {
        let task = TrackedTask::spawn(std::future::pending());
        let abort = task.abort_handle();
        abort.abort();

        assert!(task.into_join_handle().await.unwrap_err().is_cancelled());
        assert!(abort.is_finished());
    }

    #[tokio::test]
    async fn the_set_prunes_finished_tasks_on_spawn() {
        let mut set = TrackedTaskSet::default();
        set.spawn(async {});
        tokio::task::yield_now().await;
        let first = set.tasks[0].abort_handle();
        while !first.is_finished() {
            tokio::task::yield_now().await;
        }

        set.spawn(std::future::pending());
        assert_eq!(set.len(), 1);
    }

    #[tokio::test]
    async fn dropping_the_set_aborts_its_tasks() {
        let mut set = TrackedTaskSet::default();
        let abort = set.spawn(std::future::pending());
        drop(set);

        while !abort.is_finished() {
            tokio::task::yield_now().await;
        }
    }
}
