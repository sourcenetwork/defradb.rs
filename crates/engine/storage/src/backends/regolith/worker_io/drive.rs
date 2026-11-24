//! Driving a queue of one's own from the task that waits on it, for a
//! caller that is not a pool worker.

use std::future::{poll_fn, Future};
use std::pin::pin;
use std::task::Poll;

use regolith::{IoBudget, IoQueue};

/// Runs `future`, whose waits are recorded on `queue`, inside the calling
/// task: each time the future cannot go on, the task polls `queue` (doing
/// its reads on this thread), and when neither can it registers its waker on
/// the queue and returns `Pending`. It never parks the thread, so it works
/// under any executor, a single-threaded one included, and the executor
/// stays free to run the task's own cooperative scheduling.
pub(crate) async fn drive<F>(queue: &mut IoQueue, future: F) -> F::Output
where
    F: Future,
{
    let mut future = pin!(future);
    poll_fn(|cx| loop {
        if let Poll::Ready(output) = future.as_mut().poll(cx) {
            return Poll::Ready(output);
        }
        let progress = queue.poll(IoBudget::ALL);
        if progress.completed > 0 {
            // A completion woke the future: run it on now.
            continue;
        }
        if progress.wake_in.is_some() {
            // vertexia: a timed wait has no timer off a worker, so the task
            // asks to run again; the store's retry policy sets no backoff,
            // so this is never reached today. A timer here if one is set.
            cx.waker().wake_by_ref();
            return Poll::Pending;
        }
        queue.idle_waker(cx.waker());
        return Poll::Pending;
    })
    .await
}
