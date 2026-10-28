//! The handle's promises: spawn and block, deadlines, panic containment,
//! scopes and services, and the shutdown order.

mod block_on;
mod deadline;
mod panic;
mod scope;
mod shutdown;
mod spawn;

use std::future::Future;
use std::num::NonZeroUsize;
use std::pin::pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll, Waker};
use std::time::Duration;

use crate::{EdgeConfig, EdgeRuntime};

fn runtime() -> EdgeRuntime {
    runtime_with(EdgeConfig {
        drain: Duration::from_secs(5),
        stop: Duration::from_secs(1),
        ..EdgeConfig::default()
    })
}

fn runtime_with(config: EdgeConfig) -> EdgeRuntime {
    EdgeRuntime::start(EdgeConfig {
        threads: NonZeroUsize::new(2),
        thread_name: "edge-unit".to_string(),
        ..config
    })
    .expect("the runtime starts")
}

/// Sets its flag when the work holding it is dropped, whether it finished
/// or was stopped.
struct DropFlag(Arc<AtomicBool>);

impl DropFlag {
    fn new() -> (Self, Arc<AtomicBool>) {
        let flag = Arc::new(AtomicBool::new(false));
        (Self(Arc::clone(&flag)), flag)
    }
}

impl Drop for DropFlag {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

/// Polls `future` once, without an executor.
fn poll_once<F>(future: F) -> Poll<F::Output>
where
    F: Future,
{
    pin!(future).poll(&mut Context::from_waker(Waker::noop()))
}

/// A nuclei runtime of one thread, the one that calls its `block_on`.
fn single_threaded() -> nuclei::Runtime {
    nuclei::Runtime::builder()
        .worker_threads(0)
        .build()
        .expect("a single-threaded runtime starts")
}

async fn explode() -> u32 {
    panic!("boom")
}
