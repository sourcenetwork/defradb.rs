//! The background pass that rebuilds HNSW graphs when their waste says so.

use std::sync::Arc;

use storage::corekv::Store;
use tokio::task::JoinHandle;
use tokio::time::{self, MissedTickBehavior};

use crate::database::DB;
use crate::index::vector::index::VectorIndex;
use schema::VectorAlgorithm;

/// How often the sweep looks for due rebuilds. A due check is one meta read
/// per HNSW index, so a short interval would be cheap; this stays deliberate
/// because a rebuild that fires is anything but.
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

    /// Rebuilds every HNSW index whose engine says a rebuild is due.
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
                if vector.algorithm != VectorAlgorithm::Hnsw {
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
}
