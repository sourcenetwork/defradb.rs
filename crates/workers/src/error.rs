//! The pool's one error type.

use std::fmt;

/// What can go wrong submitting to, waiting on or running the pool.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Error {
    /// A blocking wait was called on a worker thread, where it would wait on
    /// the very thread that has to finish the work.
    OnWorker,
    /// A worker-local call (`spawn_local`, `sleep`) was made off a worker.
    NotOnWorker,
    /// The pool is shut down, or shut down before the task ran or finished.
    ShutDown,
    /// The task panicked; the worker went on.
    Panicked {
        /// The task's name.
        task: &'static str,
        /// The panic message.
        message: String,
    },
    /// A target named a worker the pool does not have.
    NoSuchWorker {
        /// The index asked for.
        index: usize,
        /// How many workers there are.
        workers: usize,
    },
    /// A target named a node with no worker.
    NoSuchNode(usize),
    /// A wait passed its deadline.
    TimedOut,
    /// The configuration cannot be run.
    Config(String),
    /// A thread could not be started.
    Spawn(String),
    /// A completion was polled again after it returned its result.
    Consumed,
    /// A submission that may not wait found no room on the worker.
    Full,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::OnWorker => f.write_str(
                "a blocking wait was called on a worker thread; await the completion instead",
            ),
            Error::NotOnWorker => f.write_str("this call works only on a worker thread"),
            Error::ShutDown => f.write_str("the worker pool is shut down"),
            Error::Panicked { task, message } => write!(f, "task {task} panicked: {message}"),
            Error::NoSuchWorker { index, workers } => {
                write!(f, "worker {index} does not exist; the pool has {workers}")
            }
            Error::NoSuchNode(node) => write!(f, "no worker runs on node {node}"),
            Error::TimedOut => f.write_str("the wait timed out"),
            Error::Config(reason) => write!(f, "invalid worker pool configuration: {reason}"),
            Error::Spawn(reason) => write!(f, "could not start a pool thread: {reason}"),
            Error::Consumed => f.write_str("the completion already returned its result"),
            Error::Full => f.write_str("the worker has no room for more work now; try again"),
        }
    }
}

impl std::error::Error for Error {}

impl Error {
    /// Logs that `work` did not finish on the pool because of this error (a
    /// warning while the pool shuts down, an error otherwise) and returns the
    /// one sentence its caller is told.
    pub fn report(&self, work: &str) -> &'static str {
        match self {
            Error::ShutDown => {
                tracing::warn!(work, "work did not run: the worker pool is shut down");
                "the node is shutting down; try again"
            }
            error => {
                tracing::error!(work, %error, "work failed on the worker pool");
                "the request failed on an internal error; see the node's log"
            }
        }
    }
}
