//! The handle's promises: spawn and block, deadlines, panic containment,
//! scopes and services, and the shutdown order.

mod block_on;
mod deadline;
mod panic;
mod scope;
mod shutdown;
mod spawn;
mod task_set;

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

/// Whether `flag` is set within a few seconds. A task woken after its
/// runtime stopped is dropped by whichever thread holds its shard's inbox,
/// which may be another thread, just after the wake returns.
fn set_soon(flag: &AtomicBool) -> bool {
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    let mut backoff = Duration::from_millis(1);
    while !flag.load(Ordering::SeqCst) {
        if std::time::Instant::now() > deadline {
            return false;
        }
        std::thread::sleep(backoff);
        backoff = (backoff * 2).min(Duration::from_millis(50));
    }
    true
}

/// Work that stays stuck while the test holds the sender. A detached task
/// nothing can wake, such as one awaiting `pending()`, is reclaimed by
/// nuclei, so stuck work is a wait something could still end.
fn stuck() -> (
    futures::channel::oneshot::Sender<()>,
    impl Future<Output = ()>,
) {
    let (hold, wait) = futures::channel::oneshot::channel();
    (hold, async move {
        let _ = wait.await;
    })
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
