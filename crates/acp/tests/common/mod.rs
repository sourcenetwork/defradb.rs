use std::future::Future;

pub fn on_pool<F, Fut>(body: F)
where
    F: FnOnce(workers::Pool) -> Fut + Send + 'static,
    Fut: Future<Output = ()> + 'static,
{
    let pool = storage::start_pool(workers::Config::unplaced(4), None).unwrap();
    let handle = pool.clone();
    pool.submit_and_wait(workers::Target::Worker(0), move || body(handle))
        .unwrap();
    pool.shutdown_blocking().unwrap();
}
