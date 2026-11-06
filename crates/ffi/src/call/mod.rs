//! How a C call runs its work: on a node's worker pool, with no worker ever
//! waiting for a C caller.
//!
//! - A synchronous call ([`run`], [`wait`]) parks only the C caller's own
//!   thread until its one completion, with the pool's blocking wait for plain
//!   threads. Made on a worker thread (a callback of an asynchronous call that
//!   calls back in), it is refused: the wait would stop the very worker that
//!   has to make progress.
//! - An asynchronous call ([`detach`]) returns once the pool admitted its
//!   work, and its result reaches the caller's [`DefraResultCallback`]
//!   exactly once ([`Reply`]), called on the worker that finished the work.
//!   Made on a worker thread, its admission waits as a task of that worker
//!   instead of blocking it.
//!
//! What must never happen: a worker blocked on a C caller; a callback called
//! twice, or never.

mod reply;
#[cfg(test)]
mod tests;

use std::future::Future;

use workers::{Pool, Target};

pub use reply::DefraResultCallback;
pub(crate) use reply::Reply;

use crate::state::NODES;
use crate::ERR_INVALID_NODE_HANDLE;

/// What a synchronous call made on a worker thread reports.
const ERR_ON_WORKER: &str = "a synchronous call cannot wait on a DefraDB thread: \
     call it from your own thread, or use its asynchronous form";

/// The caller's message for why the pool did not run a call.
fn unavailable(error: workers::Error) -> String {
    match error {
        workers::Error::OnWorker => ERR_ON_WORKER.to_string(),
        other => format!("the call did not run: {}", other),
    }
}

/// Runs the future `make` builds on node `node_ptr`'s worker pool, parking
/// the C caller's thread until it completes. A database that runs without a
/// pool runs it on the caller's thread.
pub(crate) fn on_node<M, Fut, T>(node_ptr: usize, make: M) -> Result<T, String>
where
    M: FnOnce() -> Fut + Send + 'static,
    Fut: Future<Output = Result<T, String>> + 'static,
    T: Send + 'static,
{
    let pool = NODES
        .get(node_ptr, |state| state.database.pool().cloned())
        .ok_or_else(|| ERR_INVALID_NODE_HANDLE.to_string())?;
    run(pool.as_ref(), Target::LeastLoaded, make)
}

/// Runs the future `make` builds for a call on `target`: in a transaction on
/// the caller's thread, since the transaction registry routes the work to
/// the transaction's own worker; on a node, on the node's worker pool.
pub(crate) fn on_target<M, Fut, T>(target: &crate::target::Target, make: M) -> Result<T, String>
where
    M: FnOnce() -> Fut + Send + 'static,
    Fut: Future<Output = Result<T, String>> + 'static,
    T: Send + 'static,
{
    match target {
        crate::target::Target::Txn(_) => wait(make()),
        crate::target::Target::Node(node) => on_node(*node, make),
    }
}

/// Runs the future `make` builds on the worker of `pool` that `target`
/// names (on the caller's thread without a pool), parking the C caller's
/// thread until it completes.
pub(crate) fn run<M, Fut, T>(pool: Option<&Pool>, target: Target, make: M) -> Result<T, String>
where
    M: FnOnce() -> Fut + Send + 'static,
    Fut: Future<Output = Result<T, String>> + 'static,
    T: Send + 'static,
{
    wait(async move {
        workers::run_on(pool, target, make)
            .await
            .map_err(unavailable)?
    })
}

/// Parks the C caller's thread until `future` completes. For a call whose
/// future submits its own work to the pool (a pooled query executor), so
/// that the work crosses into the pool once.
pub(crate) fn wait<F, T>(future: F) -> Result<T, String>
where
    F: Future<Output = Result<T, String>>,
{
    workers::block_on(future).map_err(unavailable)?
}

/// Lets `start`, an asynchronous call's admission, run to its end without
/// the caller waiting for the call's result: on a plain thread the caller
/// waits for room in the pool, if it must; on a worker thread (a callback
/// of another call that calls back in) the wait becomes a task of that
/// worker, which it never blocks.
pub(crate) fn detach<F>(start: F)
where
    F: Future<Output = ()> + 'static,
{
    let started = if workers::on_worker() {
        workers::spawn_local(start).map(drop)
    } else {
        workers::block_on(start)
    };
    if let Err(error) = started {
        tracing::warn!(%error, "an asynchronous call was not admitted");
    }
}
