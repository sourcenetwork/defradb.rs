//! The durable registrations' transitions run on the database's worker pool,
//! except the periodic sweep, which runs on the pool's blocking threads.
//! The receiver's registrations and their discharges call the store from the
//! network edge, and each call crosses to a worker once, runs its engine
//! transaction there and hands its result back. Called on a worker, or
//! without a pool, a call runs where it is made.

use std::sync::Arc;

use super::PendingDagStorage;
#[cfg(not(target_arch = "wasm32"))]
use {
    super::{Admission, PersistedPendingDag, PersistedQuarantinedDag, PersistedView, Registration},
    crate::error::{Error, Result},
    async_trait::async_trait,
    cid::Cid,
    std::future::Future,
};

/// `inner` as the sync manager holds it: on `pool` when the database has
/// one, as it is otherwise.
pub fn pooled_pending_store(
    inner: Arc<dyn PendingDagStorage>,
    pool: Option<&workers::Pool>,
) -> Arc<dyn PendingDagStorage> {
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(pool) = pool {
        return Arc::new(PooledPendingStore {
            inner,
            pool: pool.clone(),
        });
    }
    // A browser's one worker is the page's thread: a call runs where it is
    // made.
    #[cfg(target_arch = "wasm32")]
    let _ = pool;
    inner
}

/// `inner`'s transitions, run on a pool's workers; the sweep uses blocking
/// threads.
#[cfg(not(target_arch = "wasm32"))]
struct PooledPendingStore {
    inner: Arc<dyn PendingDagStorage>,
    pool: workers::Pool,
}

#[cfg(not(target_arch = "wasm32"))]
impl PooledPendingStore {
    /// Runs `call` on `inner` on the least-loaded worker.
    async fn on_pool<T, F, Fut>(&self, call: F) -> Result<T>
    where
        F: FnOnce(Arc<dyn PendingDagStorage>) -> Fut + Send + 'static,
        Fut: Future<Output = Result<T>> + 'static,
        T: Send + 'static,
    {
        let inner = Arc::clone(&self.inner);
        workers::run_on(Some(&self.pool), workers::Target::LeastLoaded, move || {
            call(inner)
        })
        .await
        .map_err(|error| Error::Storage(error.report("pending DAG transition").to_string()))?
    }

    /// Runs `call` off the worker pool on its blocking threads. The periodic
    /// pending-DAG sweep is long enough to starve the worker-poll limit, so
    /// it belongs there.
    async fn on_blocking<T, F, Fut>(&self, call: F) -> Result<T>
    where
        F: FnOnce(Arc<dyn PendingDagStorage>) -> Fut + Send + 'static,
        Fut: Future<Output = Result<T>> + 'static,
        T: Send + 'static,
    {
        let inner = Arc::clone(&self.inner);
        match self
            .pool
            .spawn_blocking(move || workers::block_on(call(inner)))
            .await
        {
            Ok(completion) => match completion.await {
                Ok(result) => result.map_err(|error| {
                    Error::Storage(error.report("pending DAG transition").to_string())
                })?,
                Err(error) => Err(Error::Storage(
                    error.report("pending DAG transition").to_string(),
                )),
            },
            Err(error) => Err(Error::Storage(
                error.report("pending DAG transition").to_string(),
            )),
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[async_trait]
impl PendingDagStorage for PooledPendingStore {
    async fn register(
        &self,
        registration: Registration<'_>,
        view: &Arc<PersistedView>,
    ) -> Result<Admission> {
        let (root, capacity) = (registration.root, registration.capacity);
        let parents = registration.parents.to_vec();
        let peer_capacity = registration.peer_capacity;
        let (record, view) = (registration.record.clone(), Arc::clone(view));
        self.on_pool(move |inner| async move {
            let registration = Registration {
                root,
                parents: &parents,
                record: &record,
                capacity,
                peer_capacity,
            };
            inner.register(registration, &view).await
        })
        .await
    }

    async fn remove(&self, root_cid: &Cid, view: &Arc<PersistedView>) -> Result<bool> {
        let (root, view) = (*root_cid, Arc::clone(view));
        self.on_pool(move |inner| async move { inner.remove(&root, &view).await })
            .await
    }

    async fn quarantine(
        &self,
        root_cid: &Cid,
        entry: &PersistedQuarantinedDag,
        view: &Arc<PersistedView>,
    ) -> Result<bool> {
        let (root, entry, view) = (*root_cid, entry.clone(), Arc::clone(view));
        self.on_pool(move |inner| async move { inner.quarantine(&root, &entry, &view).await })
            .await
    }

    async fn sweep(&self, view: &Arc<PersistedView>) -> Result<Vec<(Cid, PersistedPendingDag)>> {
        let view = Arc::clone(view);
        self.on_blocking(move |inner| async move { inner.sweep(&view).await })
            .await
    }

    async fn put(&self, root_cid: &Cid, record: &PersistedPendingDag) -> Result<()> {
        let (root, record) = (*root_cid, record.clone());
        self.on_pool(move |inner| async move { inner.put(&root, &record).await })
            .await
    }

    async fn load_all(&self) -> Result<Vec<(Cid, PersistedPendingDag)>> {
        self.on_pool(|inner| async move { inner.load_all().await })
            .await
    }

    async fn is_quarantined(&self, root_cid: &Cid) -> Result<bool> {
        let root = *root_cid;
        self.on_pool(move |inner| async move { inner.is_quarantined(&root).await })
            .await
    }

    async fn load_quarantined(&self) -> Result<Vec<(Cid, PersistedQuarantinedDag)>> {
        self.on_pool(|inner| async move { inner.load_quarantined().await })
            .await
    }

    async fn remove_quarantined(&self, root_cid: &Cid) -> Result<()> {
        let root = *root_cid;
        self.on_pool(move |inner| async move { inner.remove_quarantined(&root).await })
            .await
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use std::sync::atomic::{AtomicBool, Ordering};

    use super::*;

    /// Records whether its calls ran on a pool worker, and where the sweep ran.
    struct WhereCalled {
        worker: AtomicBool,
        sweep_worker: AtomicBool,
    }

    impl WhereCalled {
        fn new() -> Self {
            Self {
                worker: AtomicBool::new(false),
                sweep_worker: AtomicBool::new(false),
            }
        }

        fn note(&self) {
            self.worker.store(workers::on_worker(), Ordering::SeqCst);
        }

        fn note_sweep(&self) {
            let on_worker = workers::on_worker();
            self.worker.store(on_worker, Ordering::SeqCst);
            self.sweep_worker.store(on_worker, Ordering::SeqCst);
        }
    }

    #[async_trait]
    impl PendingDagStorage for WhereCalled {
        async fn register(&self, _: Registration<'_>, _: &Arc<PersistedView>) -> Result<Admission> {
            self.note();
            Ok(Admission::AtCapacity)
        }
        async fn remove(&self, _: &Cid, _: &Arc<PersistedView>) -> Result<bool> {
            self.note();
            Ok(false)
        }
        async fn quarantine(
            &self,
            _: &Cid,
            _: &PersistedQuarantinedDag,
            _: &Arc<PersistedView>,
        ) -> Result<bool> {
            self.note();
            Ok(false)
        }
        async fn sweep(&self, _: &Arc<PersistedView>) -> Result<Vec<(Cid, PersistedPendingDag)>> {
            self.note_sweep();
            Ok(Vec::new())
        }
        async fn put(&self, _: &Cid, _: &PersistedPendingDag) -> Result<()> {
            self.note();
            Ok(())
        }
        async fn load_all(&self) -> Result<Vec<(Cid, PersistedPendingDag)>> {
            self.note();
            Ok(Vec::new())
        }
        async fn is_quarantined(&self, _: &Cid) -> Result<bool> {
            self.note();
            Ok(false)
        }
        async fn load_quarantined(&self) -> Result<Vec<(Cid, PersistedQuarantinedDag)>> {
            self.note();
            Ok(Vec::new())
        }
        async fn remove_quarantined(&self, _: &Cid) -> Result<()> {
            self.note();
            Ok(())
        }
    }

    #[test]
    fn a_sweep_runs_on_blocking_threads_and_other_calls_run_on_workers() {
        let pool = workers::Pool::start(workers::Config::unplaced(1)).unwrap();
        let inner = Arc::new(WhereCalled::new());
        let store = pooled_pending_store(inner.clone(), Some(&pool));
        let view = Arc::new(PersistedView::default());
        workers::block_on(async {
            store.sweep(&view).await.unwrap();
        })
        .unwrap();
        assert!(
            !inner.sweep_worker.load(Ordering::SeqCst),
            "the sweep ran off worker polls"
        );
        workers::block_on(async {
            store.remove_quarantined(&Cid::default()).await.unwrap();
        })
        .unwrap();
        assert!(
            inner.worker.load(Ordering::SeqCst),
            "non-sweep calls run on workers"
        );
        pool.shutdown_blocking().unwrap();
    }
}
