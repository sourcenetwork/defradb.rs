//! The edge's snapshot of the node's NAC grants.
//!
//! A permission check at the network edge used to open a read scope and load
//! the grants from storage on every request. The manager now keeps an
//! in-memory copy of the committed NAC state and answers edge checks from it.
//! Every write the manager commits (a grant, a revocation, enable, disable,
//! purge, startup) builds a fresh copy and publishes it before the write
//! returns, so a request that starts after a revocation has returned is
//! refused. A request racing the revocation may still see the grants from
//! before it; the write it makes is checked again inside its own transaction.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use acp::{MemoryZanzibarStore, ZanzibarStore};
use datastore::NamespaceView;
use defra_core::thread_bounds::MaybeSendSync;
use regolith::sync::Atom;
use storage::corekv::{make_async_callback, OnCommit};

use super::{Error, NacManager, Result};

/// A manager over the in-memory copy of the NAC state.
pub(super) type Snapshot = Arc<NacManager<MemoryZanzibarStore>>;

/// The published snapshot and the refresh that built it.
#[derive(Clone)]
struct Published {
    generation: u64,
    grants: Option<Snapshot>,
}

/// The snapshot edge checks read, swapped whole by each refresh.
pub(super) struct EdgeGrants {
    published: Atom<Published>,
    next: AtomicU64,
}

impl EdgeGrants {
    /// No snapshot yet: checks read storage until the first refresh.
    pub(super) fn new() -> Self {
        Self {
            published: Atom::new(Published {
                generation: 0,
                grants: None,
            }),
            next: AtomicU64::new(0),
        }
    }

    /// The snapshot to check against, if one is published.
    pub(super) fn current(&self) -> Option<Snapshot> {
        self.published.load().grants.clone()
    }

    /// Withdraw the snapshot: checks read storage until the next refresh.
    fn withdraw(&self) {
        let generation = self.next.fetch_add(1, Ordering::SeqCst) + 1;
        self.publish(generation, None);
    }

    /// Publish `grants` unless a refresh that started later already did. A
    /// refresh that started later read storage after this one began, so it saw
    /// at least the writes this one saw.
    fn publish(&self, generation: u64, grants: Option<Snapshot>) {
        self.published.rcu(|current| {
            if current.generation < generation {
                Published {
                    generation,
                    grants: grants.clone(),
                }
            } else {
                current.clone()
            }
        });
    }
}

impl<S> NacManager<S>
where
    S: ZanzibarStore + ?Sized + 'static,
{
    /// Read the committed NAC state into a new snapshot and publish it. When
    /// the read fails the snapshot is withdrawn, and checks read storage again
    /// until the next refresh.
    pub(super) async fn refresh_edge(&self) {
        let generation = self.edge.next.fetch_add(1, Ordering::SeqCst) + 1;
        let grants = match self.read_edge_grants().await {
            Ok(grants) => grants.map(Arc::new),
            Err(error) => {
                tracing::warn!(%error, "NAC edge snapshot refresh failed; checks read storage");
                None
            }
        };
        self.edge.publish(generation, grants);
    }

    /// A grant written inside a database transaction commits with it, not
    /// here: withdraw the snapshot now, so checks read storage, and build a
    /// new one once the transaction commits. The commit awaits the refresh
    /// before it returns. An aborted transaction leaves the snapshot
    /// withdrawn, which only costs the storage read until the next write.
    pub(super) async fn refresh_edge_on_commit(&self, view: &NamespaceView) -> Result<()>
    where
        S: MaybeSendSync,
    {
        self.edge.withdraw();
        let manager = NacManager {
            nac: Arc::clone(&self.nac),
            config: self.config.clone(),
            transaction_store_bound: self.transaction_store_bound,
            edge: Arc::clone(&self.edge),
        };
        let refresh = make_async_callback(move || async move { manager.refresh_edge().await });
        view.on_commit(OnCommit::Async(refresh))
            .await
            .map_err(|error| Error::Other(error.to_string()))
    }

    /// A manager over a copy of the committed NAC state, read in one scope.
    /// `None` when the store has no scopes: its checks already read memory.
    async fn read_edge_grants(&self) -> Result<Option<NacManager<MemoryZanzibarStore>>> {
        let scope = self
            .nac
            .begin_scope(true)
            .await
            .map_err(|error| Error::Acp(error.to_string()))?;
        let Some(scope) = scope else {
            return Ok(None);
        };
        let copy = Arc::new(MemoryZanzibarStore::new());
        self.nac
            .scoped(scope.store())
            .copy_into(copy.as_ref())
            .await
            .map_err(|error| Error::Acp(error.to_string()))?;
        scope
            .commit()
            .await
            .map_err(|error| Error::Acp(error.to_string()))?;
        let snapshot = self.scoped(copy);
        snapshot
            .nac
            .load()
            .await
            .map_err(|error| Error::Acp(error.to_string()))?;
        Ok(Some(snapshot))
    }
}
