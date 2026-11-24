use std::future::Future;

use regolith::sync::Lazy;

const WORKERS: usize = 4;

/// The process's default pool, started the way a node starts its own: with
/// worker I/O, so transactions commit through `commit_nowait` on a worker's
/// queue.
static POOL: Lazy<workers::Pool> = Lazy::new(|| {
    crate::backends::start_pool(workers::Config::unplaced(WORKERS), None)
        .expect("start the test pool")
});

/// Runs a test body on a worker of the WorkerIo pool and returns its output.
/// A test that opens stores or spawns transactions wraps its body in this
/// instead of `workers::block_on`.
///
/// ```ignore
/// #[test]
/// fn my_test() {
///     test_suite::on_workers(|| async {
///         let store = RegolithStore::in_memory().unwrap();
///         // ...
///     });
/// }
/// ```
pub fn on_workers<F, Fut>(body: F) -> Fut::Output
where
    F: FnOnce() -> Fut + Send + 'static,
    Fut: Future + 'static,
    Fut::Output: Send + 'static,
{
    POOL.submit_and_wait(workers::Target::LeastLoaded, body)
        .expect("the test body ran on the pool")
}
