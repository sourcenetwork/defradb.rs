//! Spawning from anywhere: a future that runs to its end whether or not
//! anyone awaits it (a periodic job, a backfill, the callbacks of a
//! transaction dropped where nothing can await).
//!
//! Called on a worker, the work is a task of that worker: it stays on the
//! core that started it. Called anywhere else, it goes to the pool the
//! caller names, or to the process's default pool ([`default_pool`]). A
//! submission that finds no room waits for it on the calling plain thread,
//! so spawned work is never dropped.

use std::future::Future;

use super::{default_pool, Pool, Submit, Target};
use crate::Error;

/// Runs the future `make` builds to its end: on the calling worker when
/// called on one; otherwise on `pool`, or on the process's default pool
/// ([`default_pool`]) when `pool` is `None`. Returns its completion; dropping
/// it leaves the work running.
/// Errors only when no pool can take it: the pool shut down, or no pool can
/// start on this target.
///
/// ```no_run
/// # async fn demo(pool: &workers::Pool) -> Result<(), workers::Error> {
/// let done = workers::spawn(Some(pool), "backfill", || async { 7 })?;
/// assert_eq!(done.await?, 7);
/// # Ok(())
/// # }
/// ```
pub fn spawn<F, Fut>(
    pool: Option<&Pool>,
    name: &'static str,
    make: F,
) -> Result<crate::Completion<Fut::Output>, Error>
where
    F: FnOnce() -> Fut + Send + 'static,
    Fut: Future + 'static,
    Fut::Output: Send + 'static,
{
    if crate::worker::on_worker() {
        return crate::worker::spawn_named(name, make());
    }
    let options = Submit {
        name: Some(name),
        bytes: None,
    };
    match pool {
        Some(pool) => submit_now(pool, Target::LeastLoaded, options, make),
        None => submit_now(&default_pool()?, Target::LeastLoaded, options, make),
    }
}

/// Runs the future `make` builds on the pool and hands its output back: in
/// place when the caller is already a worker (no hop from worker to worker),
/// on the worker `target` names otherwise (one crossing there, one back), and
/// in place when there is no pool (a test, a one-shot tool). What a boundary
/// wraps a data-path call in. Dropped before the worker starts it, the call
/// never runs; once started, it runs to its end
/// ([`Completion::cancel_on_drop`](crate::Completion::cancel_on_drop)).
///
/// ```no_run
/// # async fn demo(pool: Option<&workers::Pool>) -> Result<(), workers::Error> {
/// let rows = workers::run_on(pool, workers::Target::LeastLoaded, || async { 3 }).await?;
/// assert_eq!(rows, 3);
/// # Ok(())
/// # }
/// ```
pub async fn run_on<F, Fut>(
    pool: Option<&Pool>,
    target: Target,
    make: F,
) -> Result<Fut::Output, Error>
where
    F: FnOnce() -> Fut + Send + 'static,
    Fut: Future + 'static,
    Fut::Output: Send + 'static,
{
    match pool {
        Some(pool) if !crate::worker::on_worker() => {
            pool.submit(target, make).await?.cancel_on_drop().await
        }
        _ => Ok(make().await),
    }
}

/// [`spawn`] for work nobody awaits (a periodic job, a backfill, the
/// callbacks of a transaction dropped where nothing can await).
///
/// ```no_run
/// # fn demo(pool: &workers::Pool) -> Result<(), workers::Error> {
/// workers::spawn_detached(Some(pool), "redrive", || async {
///     // one page per transaction, yielding between pages
/// })?;
/// # Ok(())
/// # }
/// ```
pub fn spawn_detached<F, Fut>(pool: Option<&Pool>, name: &'static str, make: F) -> Result<(), Error>
where
    F: FnOnce() -> Fut + Send + 'static,
    Fut: Future<Output = ()> + 'static,
{
    spawn(pool, name, make).map(drop)
}

/// Submits `make` from a caller that cannot await (off any worker): with
/// room now, at once; without, waiting for room on the calling plain thread.
pub(super) fn submit_now<F, Fut>(
    pool: &Pool,
    target: Target,
    options: Submit,
    make: F,
) -> Result<crate::Completion<Fut::Output>, Error>
where
    F: FnOnce() -> Fut + Send + 'static,
    Fut: Future + 'static,
    Fut::Output: Send + 'static,
{
    match pool
        .handle
        .shared
        .admit_now(target, std::mem::size_of::<F>())?
    {
        Some(admitted) => Ok(Pool::launch(target, options, admitted, make)),
        None => wait_for_room(pool, target, options, make),
    }
}

/// No room now: wait for it on this plain thread, which is never a worker.
#[cfg(not(target_family = "wasm"))]
fn wait_for_room<F, Fut>(
    pool: &Pool,
    target: Target,
    options: Submit,
    make: F,
) -> Result<crate::Completion<Fut::Output>, Error>
where
    F: FnOnce() -> Fut + Send + 'static,
    Fut: Future + 'static,
    Fut::Output: Send + 'static,
{
    crate::completion::block_on(pool.submit_with(target, options, make))?
}

/// A browser has no thread to wait on.
#[cfg(target_family = "wasm")]
fn wait_for_room<F, Fut>(
    _pool: &Pool,
    _target: Target,
    _options: Submit,
    _make: F,
) -> Result<crate::Completion<Fut::Output>, Error>
where
    F: FnOnce() -> Fut + Send + 'static,
    Fut: Future + 'static,
    Fut::Output: Send + 'static,
{
    Err(Error::Full)
}

#[cfg(all(test, not(target_family = "wasm")))]
mod tests {
    use std::future::Future;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{mpsc, Arc};
    use std::task::{Context, Waker};
    use std::time::Duration;

    use regolith::sync::Event;

    use super::run_on;
    use crate::{block_on, Config, Pool, Target};

    const LIMIT: Duration = Duration::from_secs(10);

    /// Polls `call` once, which submits it, and hands it back to be dropped.
    fn submitted<F: Future>(call: F) -> std::pin::Pin<Box<F>> {
        let mut call = Box::pin(call);
        let mut cx = Context::from_waker(Waker::noop());
        assert!(call.as_mut().poll(&mut cx).is_pending());
        call
    }

    #[test]
    fn a_call_dropped_before_its_worker_starts_it_never_runs() {
        let pool = Pool::start(Config::unplaced(1)).unwrap();
        let (started_tx, started) = mpsc::channel();
        let (release, held) = mpsc::channel::<()>();
        // The one worker's thread is held until `release` sends.
        let hold = block_on(pool.submit(Target::Worker(0), move || async move {
            started_tx.send(()).unwrap();
            let _ = held.recv();
        }))
        .unwrap()
        .unwrap();
        started.recv_timeout(LIMIT).unwrap();

        let ran = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&ran);
        drop(submitted(run_on(
            Some(&pool),
            Target::Worker(0),
            move || async move {
                flag.store(true, Ordering::SeqCst);
            },
        )));
        release.send(()).unwrap();
        block_on(hold).unwrap().unwrap();
        // A call queued after the dropped one runs after its turn.
        pool.submit_and_wait(Target::Worker(0), || async {})
            .unwrap();
        assert!(!ran.load(Ordering::SeqCst), "the dropped call never ran");
        pool.shutdown_blocking().unwrap();
    }

    #[test]
    fn a_call_dropped_while_it_runs_runs_to_its_end() {
        let pool = Pool::start(Config::unplaced(1)).unwrap();
        let (started_tx, started) = mpsc::channel();
        let (done_tx, done) = mpsc::channel();
        let gate = Arc::new(Event::new());
        let waits = Arc::clone(&gate);
        let call = submitted(run_on(Some(&pool), Target::Worker(0), move || async move {
            started_tx.send(()).unwrap();
            waits.wait().await;
            done_tx.send(()).unwrap();
        }));
        started.recv_timeout(LIMIT).unwrap();
        drop(call);
        gate.set();
        done.recv_timeout(LIMIT).expect("the running call finished");
        pool.shutdown_blocking().unwrap();
    }
}
