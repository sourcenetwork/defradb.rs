//! How the network runtime is built and how long it waits.

use std::fmt;
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::time::Duration;

/// Runs on each runtime thread as it starts, before the thread runs any
/// task: the placement hook (pinning, priority, memory policy).
pub type ThreadStart = Arc<dyn Fn() + Send + Sync + 'static>;

/// How the network runtime is built and how long it waits.
#[derive(Clone)]
pub struct EdgeConfig {
    /// Threads that run network tasks. `None`: one per CPU of a third of
    /// the cores, held for them alone so no worker pool plans on them
    /// ([`workers::EdgeCores`]); a count: that many threads, unpinned.
    pub threads: Option<NonZeroUsize>,
    /// The name every runtime thread carries.
    pub thread_name: String,
    /// How long a waiter waits on a completion before it gets
    /// [`EdgeError::Deadline`](crate::EdgeError::Deadline), unless the
    /// completion sets its own with [`Task::within`](crate::Task::within).
    pub deadline: Duration,
    /// How long shutdown waits for admitted work once it stops accepting.
    pub drain: Duration,
    /// How long shutdown then waits for blocking work it cannot cancel.
    pub stop: Duration,
    /// Placement hook run on each runtime thread as it starts.
    pub on_thread_start: Option<ThreadStart>,
}

impl Default for EdgeConfig {
    fn default() -> Self {
        Self {
            threads: None,
            thread_name: "defra-edge".to_string(),
            deadline: Duration::from_secs(300),
            drain: Duration::from_secs(30),
            stop: Duration::from_secs(5),
            on_thread_start: None,
        }
    }
}

impl fmt::Debug for EdgeConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EdgeConfig")
            .field("threads", &self.threads)
            .field("thread_name", &self.thread_name)
            .field("deadline", &self.deadline)
            .field("drain", &self.drain)
            .field("stop", &self.stop)
            .field("on_thread_start", &self.on_thread_start.is_some())
            .finish()
    }
}
