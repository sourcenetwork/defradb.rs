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
//!
//! Each copy also decides, with the same engine, every permission of every
//! identity its relationships name (and the owner's, and that of any other
//! identity), so an edge check is a map lookup, not an evaluation.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use acp::nac::{NacStatus, NodePermission};
use acp::{MemoryZanzibarStore, ZanzibarStore};
use datastore::NamespaceView;
use defra_core::thread_bounds::MaybeSendSync;
use identity::Did;
use rapidhash::{HashMapExt, HashSetExt, RapidHashMap, RapidHashSet};
use regolith::sync::Atom;
use storage::corekv::{make_async_callback, OnCommit};

use super::{Error, NacManager, Result};

/// A published edge snapshot.
pub(super) type Snapshot = Arc<EdgeSnapshot>;

/// An identity no relationship names: what it holds is what every unnamed
/// identity holds, the grants made to all actors.
const UNNAMED: &str = "did:key:edge-snapshot-unnamed";

/// The in-memory copy of the NAC state, and each identity's permissions
/// decided from it ahead of time, so an edge check is one map lookup.
pub(super) struct EdgeSnapshot {
    /// A manager over the copy: the engine that decided `decided`, and the
    /// one that answers when `decided` cannot.
    grants: NacManager<MemoryZanzibarStore>,
    /// `None` when deciding ahead failed; the engine answers every check.
    decided: Option<Decided>,
}

/// What each identity holds, decided by the engine over the copy.
struct Decided {
    /// NAC is on; when it is off every permission is held.
    enabled: bool,
    /// The permissions of every identity a relationship names, and the owner.
    named: RapidHashMap<Did, RapidHashSet<NodePermission>>,
    /// The permissions of every other identity.
    unnamed: RapidHashSet<NodePermission>,
}

impl EdgeSnapshot {
    /// Decide every permission of every identity `named` lists, and of the
    /// unnamed one, on `grants`.
    async fn new(grants: NacManager<MemoryZanzibarStore>, named: Vec<Did>) -> Self {
        let decided = decide(&grants, named).await;
        Self { grants, decided }
    }

    /// Whether `identity` holds `permission`: the decision made ahead of
    /// time, or the engine's over the copy when there is none (the wildcard
    /// identity, which the engine may treat apart, always asks the engine).
    pub(super) async fn check(&self, identity: &Did, permission: NodePermission) -> Result<bool> {
        if let Some(held) = self.lookup(identity, permission) {
            return Ok(held);
        }
        self.engine(identity, permission).await
    }

    /// [`Self::check`] for each of `permissions` in order, up to and
    /// including the first that is not granted.
    pub(super) async fn check_all(
        &self,
        identity: &Did,
        permissions: &[NodePermission],
    ) -> Vec<Result<bool>> {
        let mut outcomes = Vec::with_capacity(permissions.len());
        for &permission in permissions {
            let outcome = self.check(identity, permission).await;
            let granted = matches!(outcome, Ok(true));
            outcomes.push(outcome);
            if !granted {
                break;
            }
        }
        outcomes
    }

    /// The decision made ahead of time, if there is one for `identity`.
    fn lookup(&self, identity: &Did, permission: NodePermission) -> Option<bool> {
        let decided = self.decided.as_ref()?;
        if !decided.enabled {
            return Some(true);
        }
        if identity.is_wildcard() {
            return None;
        }
        let held = decided.named.get(identity).unwrap_or(&decided.unnamed);
        Some(held.contains(&permission))
    }

    /// The engine's decision over the copy.
    async fn engine(&self, identity: &Did, permission: NodePermission) -> Result<bool> {
        self.grants
            .nac
            .check_permission(identity, permission)
            .await
            .map_err(|e| Error::Acp(format!("permission check failed: {}", e)))
    }
}

/// Every permission each identity holds on `grants`, as the engine decides
/// it; `None` if the engine fails for any of them.
async fn decide(grants: &NacManager<MemoryZanzibarStore>, named: Vec<Did>) -> Option<Decided> {
    let enabled = grants.nac.status().await == NacStatus::Enabled;
    let mut identities: RapidHashSet<Did> = named
        .into_iter()
        .filter(|identity| !identity.is_wildcard())
        .collect();
    identities.extend(grants.nac.owner().await);
    let unnamed = Did::new(UNNAMED).ok()?;
    if identities.contains(&unnamed) {
        return None;
    }
    let mut decided = Decided {
        enabled,
        named: RapidHashMap::with_capacity(identities.len()),
        unnamed: held(grants, &unnamed).await?,
    };
    for identity in identities {
        let permissions = held(grants, &identity).await?;
        decided.named.insert(identity, permissions);
    }
    Some(decided)
}

/// The permissions `identity` holds on `grants`, by the engine.
async fn held(
    grants: &NacManager<MemoryZanzibarStore>,
    identity: &Did,
) -> Option<RapidHashSet<NodePermission>> {
    let mut held = RapidHashSet::new();
    for &permission in NodePermission::all() {
        if grants
            .nac
            .check_permission(identity, permission)
            .await
            .ok()?
        {
            held.insert(permission);
        }
    }
    Some(held)
}

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

/// Withdraws the edge snapshot when dropped before [`Self::refreshed`]. A NAC
/// write holds one from before its commit until its refresh has published, so
/// a caller that drops the write in between leaves checks reading storage,
/// never the snapshot from before a commit that stands. Withdrawing at the
/// drop also outranks every refresh that started before it, including one
/// that read storage before the commit.
pub(super) struct RefreshGuard<'edge> {
    edge: &'edge EdgeGrants,
    armed: bool,
}

impl RefreshGuard<'_> {
    /// The refresh published, or nothing committed: leave the snapshot alone.
    pub(super) fn refreshed(mut self) {
        self.armed = false;
    }
}

impl Drop for RefreshGuard<'_> {
    fn drop(&mut self) {
        if self.armed {
            self.edge.withdraw();
        }
    }
}

impl<S> NacManager<S>
where
    S: ZanzibarStore + ?Sized + 'static,
{
    /// A guard to hold across a NAC write's commit and its refresh.
    pub(super) fn refresh_guard(&self) -> RefreshGuard<'_> {
        RefreshGuard {
            edge: &self.edge,
            armed: true,
        }
    }

    /// The published edge snapshot, if there is one.
    pub(super) fn edge_snapshot(&self) -> Option<Snapshot> {
        self.edge.current()
    }

    /// Read the committed NAC state into a new snapshot and publish it. When
    /// the read fails the snapshot is withdrawn, and checks read storage again
    /// until the next refresh.
    pub(super) async fn refresh_edge(&self) {
        let generation = self.edge.next.fetch_add(1, Ordering::SeqCst) + 1;
        let grants = match self.read_edge_grants().await {
            Ok(grants) => grants,
            Err(error) => {
                tracing::warn!(%error, "NAC edge snapshot refresh failed; checks read storage");
                None
            }
        };
        self.edge.publish(generation, grants);
    }

    /// Reload the manager after a write it committed, and refresh the edge
    /// snapshot whatever the reload's outcome. The refresh reads the committed
    /// state itself, so a failed reload never leaves the edge answering from
    /// the copy taken before the write.
    pub(super) async fn reload_after_commit(&self, guard: RefreshGuard<'_>) -> Result<()> {
        let reloaded = self
            .nac
            .load()
            .await
            .map_err(|error| Error::Acp(error.to_string()));
        self.refresh_edge().await;
        guard.refreshed();
        reloaded
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
    async fn read_edge_grants(&self) -> Result<Option<Snapshot>> {
        let scope = self
            .nac
            .begin_scope(true)
            .await
            .map_err(|error| Error::Acp(error.to_string()))?;
        let Some(scope) = scope else {
            return Ok(None);
        };
        let copy = Arc::new(MemoryZanzibarStore::new());
        let named = self
            .nac
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
        Ok(Some(Arc::new(EdgeSnapshot::new(snapshot, named).await)))
    }
}
