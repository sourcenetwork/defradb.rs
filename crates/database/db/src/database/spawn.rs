//! Detached work of a database: a task of the worker that starts it, or,
//! off the pool, of the database's pool (the process's default pool where
//! it has none: a test, a one-shot tool).

/// Runs `future` to its end with nobody awaiting it. Failing to place it is
/// said out loud: only a stopped pool refuses work.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn spawn_task<F>(pool: Option<&workers::Pool>, name: &'static str, future: F)
where
    F: std::future::Future<Output = ()> + Send + 'static,
{
    if let Err(error) = workers::spawn_detached(pool, name, move || future) {
        tracing::error!(task = name, %error, "detached database work did not start");
    }
}

/// The browser's one worker is the page's own thread, where every task of a
/// database starts, so the work stays there; its futures are not `Send`.
#[cfg(target_arch = "wasm32")]
pub(crate) fn spawn_task<F>(_pool: Option<&workers::Pool>, name: &'static str, future: F)
where
    F: std::future::Future<Output = ()> + 'static,
{
    if let Err(error) = workers::spawn_local(future) {
        tracing::error!(task = name, %error, "detached database work did not start");
    }
}
