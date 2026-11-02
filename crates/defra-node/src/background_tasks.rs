use kovan_queue::seg_queue::SegQueue;

/// The tasks a node owns, each holding the database open until stopped.
pub struct BackgroundTasks {
    tasks: SegQueue<workers::Background>,
}

impl BackgroundTasks {
    pub(crate) fn new(tasks: impl IntoIterator<Item = workers::Background>) -> Self {
        let queue = SegQueue::new();
        for task in tasks {
            queue.push(task);
        }
        Self { tasks: queue }
    }

    pub(crate) fn push(&self, task: workers::Background) {
        self.tasks.push(task);
    }

    /// Stop and await all node-owned background tasks.
    ///
    /// Awaiting the stop is important for persistent stores: a stopped task
    /// holds the final database handle until its worker next polls it, which
    /// otherwise leaves the on-disk database lock held after close. Each wait
    /// is bounded, so a task that never polls again cannot stall shutdown.
    pub async fn shutdown(&self) {
        while let Some(task) = self.tasks.pop() {
            task.abort();
            let _ = workers::timeout(std::time::Duration::from_secs(1), task).await;
        }
    }
}

impl Drop for BackgroundTasks {
    fn drop(&mut self) {
        while let Some(task) = self.tasks.pop() {
            task.abort();
        }
    }
}
