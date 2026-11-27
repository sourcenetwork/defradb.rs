//! Background work that runs until stopped: a periodic pass, a sweep, a
//! watcher.
//!
//! It is a daemon task of one worker, started from anywhere
//! ([`spawn_background`]): pool shutdown does not wait for it, and it is
//! dropped when its worker stops. [`Background::abort`] ends it at its next
//! poll; awaiting the [`Background`] says when it ended.

use std::future::{poll_fn, Future};
use std::pin::{pin, Pin};
use std::sync::Arc;
use std::task::{Context, Poll};

use regolith::sync::Event;

use super::detached::spawn;
use super::Pool;
use crate::{Completion, Error};

/// Running background work. Dropping the handle leaves it running;
/// [`abort`](Background::abort) stops it.
#[must_use = "background work runs until aborted; keep the handle to stop it"]
pub struct Background {
    stop: Arc<Event>,
    state: State,
}

enum State {
    /// Its worker has not started it yet.
    Starting(Completion<Result<Completion<()>, Error>>),
    Running(Completion<()>),
    /// It never started, for this reason.
    Refused(Error),
    Ended,
}

/// Runs the future `make` builds in the background: on the calling worker
/// when called on one, else on `pool` (the process's default pool when
/// `None`), until
/// it ends, [`Background::abort`] is called, or its worker stops. When no
/// pool can take it (it shut down, or none can start on this target) that
/// is logged as an error, and awaiting the handle gives the reason.
///
/// ```no_run
/// # fn demo(pool: &workers::Pool) -> Result<(), workers::Error> {
/// let sweep = workers::spawn_background(Some(pool), "sweep", || async {
///     let _ = workers::every(std::time::Duration::from_secs(60), || async {
///         // one pass
///     })
///     .await;
/// });
/// sweep.abort();
/// # Ok(())
/// # }
/// ```
pub fn spawn_background<F, Fut>(pool: Option<&Pool>, name: &'static str, make: F) -> Background
where
    F: FnOnce() -> Fut + Send + 'static,
    Fut: Future<Output = ()> + 'static,
{
    let stop = Arc::new(Event::new());
    let stopped = Arc::clone(&stop);
    let state = match spawn(pool, name, move || async move {
        crate::worker::spawn_daemon_named(name, until(stopped, make()))
    }) {
        Ok(started) => State::Starting(started),
        Err(error) => {
            tracing::error!(task = name, %error, "background work did not start");
            State::Refused(error)
        }
    };
    Background { stop, state }
}

/// `job`, ended early once `stop` is set.
async fn until<Fut>(stop: Arc<Event>, job: Fut)
where
    Fut: Future<Output = ()>,
{
    let mut job = pin!(job);
    let mut stopped = pin!(stop.wait());
    poll_fn(|cx| {
        if stopped.as_mut().poll(cx).is_ready() {
            return Poll::Ready(());
        }
        job.as_mut().poll(cx)
    })
    .await;
}

impl Background {
    /// Stops the work at its next poll.
    pub fn abort(&self) {
        self.stop.set();
    }

    /// Whether the work has ended, however it ended.
    pub fn is_finished(&self) -> bool {
        match &self.state {
            State::Starting(_) => false,
            State::Running(done) => done.is_finished(),
            State::Refused(_) | State::Ended => true,
        }
    }
}

impl Future for Background {
    /// `Ok` once the work ended or was aborted; [`Error::ShutDown`] when its
    /// worker stopped first, or why it never started.
    type Output = Result<(), Error>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        loop {
            match &mut this.state {
                State::Starting(started) => match Pin::new(started).poll(cx) {
                    Poll::Pending => return Poll::Pending,
                    Poll::Ready(Ok(Ok(running))) => this.state = State::Running(running),
                    Poll::Ready(Ok(Err(error)) | Err(error)) => {
                        this.state = State::Ended;
                        return Poll::Ready(Err(error));
                    }
                },
                State::Running(done) => {
                    let outcome = std::task::ready!(Pin::new(done).poll(cx));
                    this.state = State::Ended;
                    return Poll::Ready(outcome);
                }
                State::Refused(error) => {
                    let error = error.clone();
                    this.state = State::Ended;
                    return Poll::Ready(Err(error));
                }
                State::Ended => return Poll::Ready(Err(Error::Consumed)),
            }
        }
    }
}

impl std::fmt::Debug for Background {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Background")
            .field("finished", &self.is_finished())
            .finish_non_exhaustive()
    }
}
