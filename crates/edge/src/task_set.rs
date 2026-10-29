//! Network tasks one component owns together: a server's connections, a
//! host's requests in flight.

use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

use defra_core::thread_bounds::MaybeSend;
use futures::stream::{FuturesUnordered, StreamExt};

use crate::error::EdgeError;
use crate::handle::Edge;
use crate::task::Task;

/// Network tasks one component owns together. Each runs as
/// [`Edge::spawn`] runs it, but with no deadline: it lives as long as the
/// set holds it. A task that ended is taken with
/// [`join_next`](Self::join_next); dropping the set stops the rest.
pub struct TaskSet<T> {
    edge: Edge,
    tasks: FuturesUnordered<Owned<T>>,
}

impl<T> TaskSet<T>
where
    T: MaybeSend + 'static,
{
    /// An empty set whose tasks run on `edge`.
    pub fn new(edge: Edge) -> Self {
        Self {
            edge,
            tasks: FuturesUnordered::new(),
        }
    }

    /// Runs `future` as a network task of this set.
    pub fn spawn<F>(&mut self, future: F)
    where
        F: Future<Output = T> + MaybeSend + 'static,
    {
        let task = self.edge.spawn(future).within(Duration::MAX);
        self.tasks.push(Owned(task));
    }

    /// How the next task of the set to end ended, or `None` when the set
    /// holds none. Dropping the wait loses no task.
    pub async fn join_next(&mut self) -> Option<Result<T, EdgeError>> {
        self.tasks.next().await
    }

    /// The tasks the set holds, ended ones not yet taken included.
    pub fn len(&self) -> usize {
        self.tasks.len()
    }

    /// Whether the set holds no task.
    pub fn is_empty(&self) -> bool {
        self.tasks.is_empty()
    }

    /// Stops every task of the set that has not ended; each is still taken
    /// with [`join_next`](Self::join_next), as [`EdgeError::Cancelled`].
    pub fn abort_all(&self) {
        for task in self.tasks.iter() {
            task.0.abort();
        }
    }
}

/// A task of a set, stopped when the set lets go of it.
struct Owned<T>(Task<T>);

impl<T> Future for Owned<T> {
    type Output = Result<T, EdgeError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        Pin::new(&mut self.get_mut().0).poll(cx)
    }
}

impl<T> Drop for Owned<T> {
    fn drop(&mut self) {
        self.0.abort();
    }
}
