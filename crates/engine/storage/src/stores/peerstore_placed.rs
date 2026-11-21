//! A peerstore placed on a worker pool: a caller on the network edge (the
//! retry sweep, the failure recorder, a replay's sends) runs its peerstore
//! calls there, one crossing per call ([`Peerstore::run`]).

use std::future::Future;

use super::Peerstore;
use crate::corekv::{Result, Store};

impl<S: Store> Clone for Peerstore<S> {
    fn clone(&self) -> Self {
        Self {
            store: self.store.clone(),
            retry_schedule: self.retry_schedule.clone(),
            retry: self.retry,
            pool: self.pool.clone(),
        }
    }
}

impl<S: Store> Peerstore<S> {
    /// Runs the calls made through [`run`](Self::run) on `pool`.
    pub fn placed_on(mut self, pool: Option<&workers::Pool>) -> Self {
        self.pool = pool.cloned();
        self
    }

    /// The pool it is placed on.
    pub fn pool(&self) -> Option<&workers::Pool> {
        self.pool.as_ref()
    }
}

impl<S: Store + 'static> Peerstore<S> {
    /// Runs `call` on this peerstore on the pool it is placed on: in place on
    /// a worker, or without a pool.
    #[cfg(not(target_arch = "wasm32"))]
    pub async fn run<T, F, Fut>(&self, call: F) -> Result<T>
    where
        F: FnOnce(Self) -> Fut + Send + 'static,
        Fut: Future<Output = Result<T>> + 'static,
        T: Send + 'static,
    {
        let peerstore = self.clone();
        let target = workers::Target::LeastLoaded;
        workers::run_on(self.pool.as_ref(), target, move || call(peerstore))
            .await
            .map_err(|error| match error {
                workers::Error::ShutDown => crate::corekv::Error::DBClosed,
                error => crate::corekv::Error::Backend(error.to_string()),
            })?
    }

    /// A browser's one worker is the page's thread: the call runs in place.
    #[cfg(target_arch = "wasm32")]
    pub async fn run<T, F, Fut>(&self, call: F) -> Result<T>
    where
        F: FnOnce(Self) -> Fut,
        Fut: Future<Output = Result<T>>,
    {
        call(self.clone()).await
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use std::sync::Arc;

    use crate::backends::{RegolithStore, RegolithStoreOptions};
    use crate::corekv::Store;
    use crate::stores::Peerstore;

    #[test]
    fn a_call_run_off_the_pool_is_made_on_a_worker() {
        let pool =
            workers::Pool::start(crate::with_worker_io(workers::Config::unplaced(1))).unwrap();
        let options = RegolithStoreOptions::memory().placed_on(&pool);
        let store = Arc::new(RegolithStore::open_with_options("regolith-memory", options).unwrap());
        let stats = store.transaction_stats_handle().unwrap();
        let placed = Peerstore::new(Arc::clone(&store)).placed_on(Some(&pool));
        let before = stats.snapshot().off_pool_calls;

        let read = workers::block_on(async {
            placed
                .run(|peerstore| async move { peerstore.create_replicator("peer-a", b"r").await })
                .await
                .unwrap();
            placed
                .run(|peerstore| async move { peerstore.get_replicator("peer-a").await })
                .await
                .unwrap()
        })
        .unwrap();
        assert_eq!(read.as_deref(), Some(&b"r"[..]));
        assert_eq!(
            stats.snapshot().off_pool_calls,
            before,
            "both calls ran on a worker"
        );

        // Unplaced, the call is made where it is run: off the pool, counted.
        let unplaced = Peerstore::new(Arc::clone(&store));
        workers::block_on(
            unplaced.run(|peerstore| async move { peerstore.get_replicator("peer-a").await }),
        )
        .unwrap()
        .unwrap();
        assert!(stats.snapshot().off_pool_calls > before);
        pool.shutdown_blocking().unwrap();
    }
}
