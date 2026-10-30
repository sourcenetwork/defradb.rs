//! When deferred indexes drain: at open, on each commit that owes one, and on
//! a sweep in case a request was lost with a crash.

#[cfg(not(target_arch = "wasm32"))]
use std::sync::Arc;

use storage::corekv::Store;

use super::drain::drain_index;
use super::IndexRef;
use crate::database::DB;
use crate::index::error::{Error, Result};

/// How long the drain waits for a request before it sweeps every index.
#[cfg(not(target_arch = "wasm32"))]
const SWEEP_INTERVAL: std::time::Duration = std::time::Duration::from_secs(5);

impl<S: Store + 'static> DB<S> {
    /// Drains every deferred index until nothing is owed.
    pub async fn drain_deferred_indexes(&self) -> Result<()> {
        for name in self
            .list_collections()
            .map_err(|e| Error::Other(e.to_string()))?
        {
            let Some(collection) = self
                .get_collection(&name)
                .map_err(|e| Error::Other(e.to_string()))?
            else {
                continue;
            };
            for index in collection.write_indexes() {
                if crdt::deferred_index(index, collection.schema(), collection.crdts()) {
                    while drain_index(self, &name, index.id).await? {}
                }
            }
        }
        Ok(())
    }

    /// Drains the indexes commits asked for since the last call.
    async fn drain_requested(&self) -> Result<()> {
        let requested = self.deferred_drains.take();
        if requested.is_empty() {
            return Ok(());
        }
        for name in self
            .list_collections()
            .map_err(|e| Error::Other(e.to_string()))?
        {
            let Some(collection) = self
                .get_collection(&name)
                .map_err(|e| Error::Other(e.to_string()))?
            else {
                continue;
            };
            let short_id = collection.resolved_root_id();
            for index in collection.write_indexes() {
                if requested.contains(&(short_id, index.id) as &IndexRef) {
                    while drain_index(self, &name, index.id).await? {}
                }
            }
        }
        Ok(())
    }

    /// Starts the deferred index drain on the database's pool: everything
    /// owed at open first, then each request, and a sweep when none came.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn start_deferred_index_task(self: Arc<Self>) -> workers::Background {
        use futures::future::{select, Either};

        let pool = self.pool().cloned();
        workers::spawn_background(pool.as_ref(), "db::deferred_index", move || async move {
            if let Err(error) = self.drain_deferred_indexes().await {
                tracing::warn!(%error, "deferred index settle at open failed");
            }
            loop {
                let Ok(sweep) = workers::sleep(SWEEP_INTERVAL) else {
                    tracing::error!("the deferred index drain stopped: it runs only on a worker");
                    return;
                };
                let drained =
                    match select(std::pin::pin!(self.deferred_drains.requested()), sweep).await {
                        Either::Left(_) => self.drain_requested().await,
                        Either::Right(_) => self.drain_deferred_indexes().await,
                    };
                if let Err(error) = drained {
                    tracing::warn!(%error, "deferred index drain failed; the next sweep retries");
                }
            }
        })
    }
}
