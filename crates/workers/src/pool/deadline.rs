//! Deadlines a `Send` future can carry: a delay is a timer task on a
//! worker's deadline queue, awaited through its [`Completion`], so the
//! awaiting future holds nothing tied to one thread.
//!
//! On a worker the timer is a task of that worker, so it fires on the core
//! that waits for it. Off-worker, [`delay_on`] uses its supplied pool or the
//! process default ([`default_pool`](crate::default_pool)). A timer whose
//! waiter is gone ends at once, so a deadline that was met holds no task
//! until it would have fired.

use std::future::{poll_fn, Future};
use std::pin::{pin, Pin};
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;

use regolith::sync::Event;

use super::detached::submit_now;
use super::{default_pool, Pool, Submit, Target};
use crate::{Completion, Error};

/// Completes once `duration` has passed on the default pool's clock. The
/// [`Delay`] is `Send`, so a future that must be `Send` can await it.
pub fn delay(duration: Duration) -> Result<Delay, Error> {
    delay_on(None, duration)
}

/// A delay whose off-worker timer runs on `pool`; without one it uses the
/// process default. On a worker, the timer always stays on that worker.
#[must_use = "a delay does nothing unless awaited"]
pub fn delay_on(pool: Option<&Pool>, duration: Duration) -> Result<Delay, Error> {
    let dropped = Arc::new(Event::new());
    let waiter_gone = Arc::clone(&dropped);
    let tick = move || async move {
        let Ok(sleep) = crate::worker::sleep(duration) else {
            return;
        };
        let mut sleep = pin!(sleep);
        let mut gone = pin!(waiter_gone.wait());
        poll_fn(|cx| {
            if sleep.as_mut().poll(cx).is_ready() || gone.as_mut().poll(cx).is_ready() {
                Poll::Ready(())
            } else {
                Poll::Pending
            }
        })
        .await;
    };
    // A timer has no work to finish: shutdown does not wait for one whose
    // waiter already moved on.
    let fired = if crate::worker::on_worker() {
        crate::worker::spawn_daemon_named("workers::delay", tick())?
    } else {
        let options = Submit {
            name: Some("workers::delay"),
            bytes: None,
        };
        let pool = pool.cloned().map(Ok).unwrap_or_else(default_pool)?;
        submit_now(&pool, Target::LeastLoaded, options, tick)?
    };
    Ok(Delay { fired, dropped })
}

/// The timer [`delay`] sets: completes when it fires, and ends it when
/// dropped first.
#[must_use = "a delay does nothing unless awaited"]
pub struct Delay {
    fired: Completion<()>,
    dropped: Arc<Event>,
}

impl Future for Delay {
    type Output = Result<(), Error>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        Pin::new(&mut self.get_mut().fired).poll(cx)
    }
}

impl Drop for Delay {
    fn drop(&mut self) {
        self.dropped.set();
    }
}

/// `future`'s output, or [`Error::TimedOut`] once `limit` passes first; the
/// future is dropped then.
///
/// ```no_run
/// # async fn demo() -> Result<(), workers::Error> {
/// let answer = workers::timeout(std::time::Duration::from_secs(1), async { 42 }).await?;
/// assert_eq!(answer, 42);
/// # Ok(())
/// # }
/// ```
pub async fn timeout<F>(limit: Duration, future: F) -> Result<F::Output, Error>
where
    F: Future,
{
    timeout_on(None, limit, future).await
}

/// `future`'s output, or why its deadline ended it first. The off-worker
/// timer runs on `pool`; without one it uses the process default.
pub async fn timeout_on<F>(
    pool: Option<&Pool>,
    limit: Duration,
    future: F,
) -> Result<F::Output, Error>
where
    F: Future,
{
    let mut deadline = pin!(delay_on(pool, limit)?);
    let mut future = pin!(future);
    poll_fn(|cx| {
        if let Poll::Ready(output) = future.as_mut().poll(cx) {
            return Poll::Ready(Ok(output));
        }
        match deadline.as_mut().poll(cx) {
            Poll::Ready(Ok(())) => Poll::Ready(Err(Error::TimedOut)),
            // The timer's pool stopped before it fired: no deadline is left
            // to keep, and waiting on would hang, so the wait ends here.
            Poll::Ready(Err(error)) => Poll::Ready(Err(error)),
            Poll::Pending => Poll::Pending,
        }
    })
    .await
}
