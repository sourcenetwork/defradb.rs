//! Completions: the result of one piece of network work, with a deadline.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;

use crate::error::EdgeError;
use crate::platform::{Instant, JoinHandle};
use crate::shared::Shared;
use crate::timer::Timer;

/// The completion of one piece of network work.
///
/// Awaiting it gives the work's value, or why there is none: the work
/// panicked, was stopped, the runtime was shutting down when it was
/// submitted, or its deadline passed. The deadline runs from the moment the
/// work was submitted, [`EdgeConfig::deadline`](crate::EdgeConfig::deadline)
/// unless [`within`](Self::within) sets another; when it passes, the waiter
/// gets [`EdgeError::Deadline`] and async work is stopped. Blocking work
/// cannot be stopped once it started: it runs to its end and its value is
/// dropped.
///
/// It can be awaited from any thread and any executor. Dropping it leaves
/// the work running, as a detached task, until it ends or the runtime
/// stops.
pub struct Task<T> {
    running: Option<Running<T>>,
}

struct Running<T> {
    join: JoinHandle<Result<T, EdgeError>>,
    shared: Arc<Shared>,
    /// Armed on the first poll: detached work never pays for a timer.
    timer: Option<Timer>,
    submitted: Instant,
    limit: Duration,
}

impl<T> Task<T> {
    pub(crate) fn refused() -> Self {
        Self { running: None }
    }

    pub(crate) fn running(shared: &Arc<Shared>, join: JoinHandle<Result<T, EdgeError>>) -> Self {
        Self {
            running: Some(Running {
                join,
                shared: Arc::clone(shared),
                timer: None,
                submitted: Instant::now(),
                limit: shared.deadline,
            }),
        }
    }

    /// Sets the deadline to `limit` after the work was submitted;
    /// `Duration::MAX` is none.
    pub fn within(mut self, limit: Duration) -> Self {
        if let Some(running) = &mut self.running {
            running.limit = limit;
            running.timer = None;
        }
        self
    }

    /// Stops the work if it has not finished. Its waiter gets
    /// [`EdgeError::Cancelled`].
    pub fn abort(&self) {
        if let Some(running) = &self.running {
            running.join.abort();
        }
    }

    /// Whether the work has ended, however it ended.
    pub fn is_finished(&self) -> bool {
        self.running
            .as_ref()
            .is_none_or(|running| running.join.is_finished())
    }
}

impl<T> Future for Task<T> {
    type Output = Result<T, EdgeError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let Some(running) = self.get_mut().running.as_mut() else {
            return Poll::Ready(Err(EdgeError::Closed));
        };
        if let Poll::Ready(joined) = Pin::new(&mut running.join).poll(cx) {
            return Poll::Ready(joined.and_then(|outcome| outcome));
        }
        if running.limit == Duration::MAX && !running.shared.is_stopped() {
            return Poll::Pending;
        }
        let (shared, submitted, limit) = (&running.shared, running.submitted, running.limit);
        let timer = running
            .timer
            .get_or_insert_with(|| Timer::after(shared, submitted, limit));
        timer.poll_limit(cx, limit).map(|error| {
            running.join.abort();
            Err(error)
        })
    }
}

/// A long-running piece of network work: an accept loop, a subscription,
/// an event stream. It stops when the runtime or its scope stops
/// accepting, when [`stop`](Self::stop) is called, or when the handle is
/// dropped, unless it was [`detach`](Self::detach)ed.
pub struct Service {
    task: Option<Task<()>>,
}

impl Service {
    /// A service has no deadline: it runs until it is stopped.
    pub(crate) fn new(task: Task<()>) -> Self {
        Self {
            task: Some(task.within(Duration::MAX)),
        }
    }

    /// Stops the service.
    pub fn stop(&self) {
        if let Some(task) = &self.task {
            task.abort();
        }
    }

    /// Stops the service and completes once it has ended, so whatever it
    /// held has been let go of.
    pub async fn stopped(mut self) {
        if let Some(task) = self.task.take() {
            task.abort();
            let _ = task.await;
        }
    }

    /// Whether the service is still running. A service the runtime refused
    /// because it was shutting down never runs.
    pub fn is_running(&self) -> bool {
        self.task.as_ref().is_some_and(|task| !task.is_finished())
    }

    /// Lets the service run until it ends or its runtime or scope stops
    /// accepting.
    pub fn detach(mut self) {
        self.task = None;
    }
}

impl Drop for Service {
    fn drop(&mut self) {
        self.stop();
    }
}
