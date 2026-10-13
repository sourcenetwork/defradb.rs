//! The background pass that rebuilds HNSW graphs when their waste says so,
//! and merges DiskANN indexes' pending inserts and tombstones into their graphs.

use storage::corekv::Store;

#[cfg(not(target_arch = "wasm32"))]
use std::sync::Arc;
#[cfg(not(target_arch = "wasm32"))]
use tokio::task::JoinHandle;
#[cfg(not(target_arch = "wasm32"))]
use tokio::time::{self, MissedTickBehavior};

use crate::database::DB;
use crate::index::vector::index::VectorIndex;
use schema::VectorAlgorithm;

/// How often the sweep looks for due rebuilds. A due check is one meta read
/// per HNSW index, so a short interval would be cheap; this stays deliberate
/// because a rebuild that fires is anything but.
#[cfg(not(target_arch = "wasm32"))]
const SWEEP_INTERVAL: std::time::Duration = std::time::Duration::from_secs(60);

impl<S: Store + 'static> DB<S> {
    /// Starts the vector index rebuild task.
    ///
    /// HNSW maintenance is lazy by design: updates keep the graph reachable
    /// but leave waste behind, and a whole-graph rebuild is far too heavy to
    /// price into a single write. This task owns that rebuild instead. Each
    /// sweep asks every HNSW index whether its waste says a rebuild is due:
    /// a fresh or clean graph answers one meta read and nothing more, while a
    /// graph owed one (last written before self-link-free inserts, or heavy
    /// with tombstones) is rebuilt on the sweep's own transaction.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn start_vector_rebuild_task(self: Arc<Self>) -> JoinHandle<()> {
        tokio::spawn(async move {
            let mut ticker = time::interval(SWEEP_INTERVAL);
            ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
            loop {
                ticker.tick().await;
                if let Err(error) = self.rebuild_due_vector_indexes().await {
                    tracing::warn!(error = %error, "Vector index rebuild sweep failed");
                }
            }
        })
    }

    /// Rebuilds every HNSW index whose engine says a rebuild is due, and
    /// merges every DiskANN index with anything pending.
    ///
    /// Each index rebuilds on its own transaction, committed only when it
    /// ran, so an idle sweep writes nothing. A failure is logged and skipped
    /// rather than failing the sweep: one index's store trouble must not
    /// starve the others.
    pub async fn rebuild_due_vector_indexes(&self) -> crate::error::Result<()> {
        for name in self.list_collections()? {
            let Some(collection) = self.get_collection(&name)? else {
                continue;
            };
            for desc in collection.get_indexes() {
                let Some(vector) = desc.vector() else {
                    continue;
                };
                if !matches!(
                    vector.algorithm,
                    VectorAlgorithm::Hnsw | VectorAlgorithm::DiskAnn
                ) {
                    continue;
                }
                let index = match VectorIndex::try_new(collection.resolved_root_id(), desc.clone())
                {
                    Ok(index) => index,
                    Err(error) => {
                        tracing::warn!(
                            collection = %name,
                            index = %desc.name,
                            error = %error,
                            "Skipping vector index the rebuild sweep cannot open"
                        );
                        continue;
                    }
                };
                if vector.algorithm == VectorAlgorithm::DiskAnn {
                    self.merge_diskann(&name, &desc.name, &index).await;
                    continue;
                }
                let txn = match self.new_txn(false).await {
                    Ok(txn) => txn,
                    Err(error) => {
                        tracing::warn!(
                            collection = %name,
                            index = %desc.name,
                            error = %error,
                            "Vector index rebuild sweep could not open a transaction"
                        );
                        continue;
                    }
                };
                // Vector index keys live in the datastore namespace, so the
                // engine runs on that view of the sweep's transaction.
                let mut datastore = match txn.datastore() {
                    Ok(datastore) => datastore,
                    Err(error) => {
                        let _ = txn.discard();
                        tracing::warn!(
                            collection = %name,
                            index = %desc.name,
                            error = %error,
                            "Vector index rebuild sweep could not open the datastore"
                        );
                        continue;
                    }
                };
                match index.rebuild_if_due(&mut datastore).await {
                    Ok(true) => {
                        if let Err(error) = txn.commit().await {
                            tracing::warn!(
                                collection = %name,
                                index = %desc.name,
                                error = %error,
                                "Vector index rebuild failed to commit"
                            );
                        }
                    }
                    Ok(false) => {
                        let _ = txn.discard();
                    }
                    Err(error) => {
                        let _ = txn.discard();
                        tracing::warn!(
                            collection = %name,
                            index = %desc.name,
                            error = %error,
                            "Vector index rebuild failed"
                        );
                    }
                }
            }
        }
        Ok(())
    }

    /// Merges one DiskANN index until nothing is pending, a plan at a time.
    ///
    /// Each plan is read on a read-only snapshot and applied on its own write
    /// transaction, so writers appending to the pending set meanwhile neither
    /// abort the merge nor are aborted by it. A merge that still conflicts
    /// (an update to a node it is merging) is logged and retried next sweep.
    async fn merge_diskann(&self, collection: &str, name: &str, index: &VectorIndex) {
        // A plan takes at most `MAX_PLAN_PENDING` inserts; a few rounds catch
        // up a backlog without letting one index hold the sweep indefinitely.
        const MAX_ROUNDS: usize = 8;
        for _ in 0..MAX_ROUNDS {
            let plan = {
                let snapshot = match self.new_txn(true).await {
                    Ok(txn) => txn,
                    Err(error) => {
                        tracing::warn!(collection, index = name, error = %error, "DiskANN merge could not open a snapshot");
                        return;
                    }
                };
                let planned = match snapshot.datastore() {
                    Ok(mut view) => index.merge_plan(&mut view).await,
                    Err(error) => {
                        tracing::warn!(collection, index = name, error = %error, "DiskANN merge could not open the datastore");
                        let _ = snapshot.discard();
                        return;
                    }
                };
                let _ = snapshot.discard();
                match planned {
                    Ok(Some(plan)) => plan,
                    Ok(None) => return,
                    Err(error) => {
                        tracing::warn!(collection, index = name, error = %error, "DiskANN merge could not plan");
                        return;
                    }
                }
            };

            let txn = match self.new_txn(false).await {
                Ok(txn) => txn,
                Err(error) => {
                    tracing::warn!(collection, index = name, error = %error, "DiskANN merge could not open a transaction");
                    return;
                }
            };
            let merged = match txn.datastore() {
                Ok(mut view) => index.merge(&mut view, plan).await,
                Err(error) => {
                    tracing::warn!(collection, index = name, error = %error, "DiskANN merge could not open the datastore");
                    let _ = txn.discard();
                    return;
                }
            };
            if let Err(error) = merged {
                let _ = txn.discard();
                tracing::warn!(collection, index = name, error = %error, "DiskANN merge failed");
                return;
            }
            if let Err(error) = txn.commit().await {
                tracing::warn!(collection, index = name, error = %error, "DiskANN merge failed to commit");
                return;
            }
        }
    }
}
