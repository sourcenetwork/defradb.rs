//! Per-peer selective CAR grant cache for outbound pushes.
//!
//! The cache closes the pre-send race. Post-ack/restart recovery does not rely
//! on its lifetime: the CAR serve path re-derives exact-root authority from the
//! durable replicator configuration, selected-transfer authority and DB-backed
//! root classification.

use std::hash::Hash;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use web_time::Instant;

use async_trait::async_trait;
use cid::Cid;
use defra_core::thread_bounds::MaybeSendSync;
use kovan::Atom;
use kovan_map::HopscotchMap;
use regolith::sync::OnceCell;

use crate::transport::PeerId;

/// Covers the receiver's worst-case bounded DAG fetch retry budget (roughly
/// seven minutes with four providers) after it has acknowledged the PushLog.
const POST_ACK_RECOVERY_WINDOW: Duration = Duration::from_secs(10 * 60);
const MAX_SELECTIVE_CAR_GRANTS: usize = 65_536;
const MAX_SELECTIVE_CAR_GRANTS_PER_PEER: usize = 4096;

type PeerGrants = HopscotchMap<u64, Arc<PushGrant>, rapidhash::fast::RandomState>;

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
trait SelectedCarStore: MaybeSendSync {
    async fn allows(&self, peer: &PeerId, root: &Cid) -> storage::corekv::Result<bool>;
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl<S> SelectedCarStore for storage::stores::Peerstore<S>
where
    S: storage::corekv::Store + 'static,
{
    async fn allows(&self, peer: &PeerId, root: &Cid) -> storage::corekv::Result<bool> {
        let peer = peer.as_str().to_owned();
        let root = root.to_bytes();
        self.run(move |store| async move { store.selected_car_root_allowed(&peer, &root).await })
            .await
    }
}

fn rapid_map<K, V>() -> HopscotchMap<K, V, rapidhash::fast::RandomState>
where
    K: Hash + Eq + Clone + 'static,
    V: Clone + 'static,
{
    HopscotchMap::with_hasher(rapidhash::fast::RandomState::default())
}

struct PushGrant {
    root_cid: Cid,
    /// Active pushes have no expiry. Dropping the registration starts the
    /// bounded post-ack recovery window instead of revoking access immediately.
    expires_at: Atom<Option<Instant>>,
}

impl PushGrant {
    fn is_live(&self, now: Instant) -> bool {
        self.expires_at
            .peek(|expiry| expiry.is_none_or(|expiry| expiry > now))
    }
}

pub(in crate::sync) struct SelectiveCarAccess {
    durable: OnceCell<Arc<dyn SelectedCarStore>>,
    next_id: AtomicU64,
    recovery_window: Duration,
    /// A peer's map is never removed once created: removing it races with a
    /// grant registering into that same map. A departed peer leaves one empty
    /// map behind, bounded by the peers ever pushed to.
    grants: HopscotchMap<PeerId, Arc<PeerGrants>, rapidhash::fast::RandomState>,
}

impl Default for SelectiveCarAccess {
    fn default() -> Self {
        Self::with_recovery_window(POST_ACK_RECOVERY_WINDOW)
    }
}

impl SelectiveCarAccess {
    fn with_recovery_window(recovery_window: Duration) -> Self {
        Self {
            durable: OnceCell::new(),
            next_id: AtomicU64::new(0),
            recovery_window,
            grants: rapid_map(),
        }
    }

    pub(super) fn register(
        self: &Arc<Self>,
        peer_id: PeerId,
        root_cid: Cid,
    ) -> Option<SelectiveCarAccessGuard> {
        let grant_id = self.next_id.fetch_add(1, Ordering::Relaxed);

        self.remove_expired(Instant::now());
        let peer_grants = match self.grants.get(&peer_id) {
            Some(peer_grants) => peer_grants,
            None => self
                .grants
                .get_or_insert(peer_id.clone(), Arc::new(rapid_map())),
        };
        peer_grants.insert(
            grant_id,
            Arc::new(PushGrant {
                root_cid,
                expires_at: Atom::new(None),
            }),
        );
        let mut peer_count = peer_grants.len();
        let mut total_grants = self.grant_count();
        if peer_count > MAX_SELECTIVE_CAR_GRANTS_PER_PEER || total_grants > MAX_SELECTIVE_CAR_GRANTS
        {
            self.reclaim_completed(&peer_grants);
            peer_count = peer_grants.len();
            total_grants = self.grant_count();
        }
        if peer_count > MAX_SELECTIVE_CAR_GRANTS_PER_PEER || total_grants > MAX_SELECTIVE_CAR_GRANTS
        {
            peer_grants.remove(&grant_id);
            tracing::warn!(
                peer_id = %peer_id,
                root_cid = %root_cid,
                peer_grants = peer_count - 1,
                total_grants = total_grants - 1,
                "Selective CAR authority capacity reached; retaining durable head marker"
            );
            return None;
        }

        Some(SelectiveCarAccessGuard {
            access: Arc::clone(self),
            peer_id,
            grant_id,
        })
    }

    fn grant_count(&self) -> usize {
        self.grants.values().map(|grants| grants.len()).sum()
    }

    fn reclaim_completed(&self, target_peer: &PeerGrants) {
        Self::reclaim_completed_while(target_peer, || {
            target_peer.len() > MAX_SELECTIVE_CAR_GRANTS_PER_PEER
        });
        for peer_grants in self.grants.values() {
            if self.grant_count() <= MAX_SELECTIVE_CAR_GRANTS {
                break;
            }
            Self::reclaim_completed_while(&peer_grants, || {
                self.grant_count() > MAX_SELECTIVE_CAR_GRANTS
            });
        }
    }

    fn reclaim_completed_while<F>(peer_grants: &PeerGrants, over_capacity: F)
    where
        F: Fn() -> bool,
    {
        for (grant_id, grant) in peer_grants.iter() {
            if !over_capacity() {
                break;
            }
            // Expiry is published only when the registration's guard is dropped;
            // that completed entry cannot become an active send again.
            if grant.expires_at.peek(Option::is_some) {
                peer_grants.remove(&grant_id);
            }
        }
    }

    pub(super) fn allows_root(&self, peer_id: &PeerId, root_cid: &Cid) -> bool {
        let now = Instant::now();
        self.remove_expired(now);
        self.grants.get(peer_id).is_some_and(|peer_grants| {
            peer_grants
                .values()
                .any(|grant| grant.root_cid == *root_cid && grant.is_live(now))
        })
    }

    pub(super) async fn persisted_root_allowed(&self, peer: &PeerId, root: &Cid) -> bool {
        match self.durable.get() {
            Some(store) => match store.allows(peer, root).await {
                Ok(allowed) => allowed,
                Err(error) => {
                    tracing::debug!(%error, "Selected CAR authority unavailable; checking independent authority");
                    false
                }
            },
            None => false,
        }
    }

    fn finish_push(&self, peer_id: &PeerId, grant_id: u64) {
        let Some(grant) = self
            .grants
            .get(peer_id)
            .and_then(|peer_grants| peer_grants.get(&grant_id))
        else {
            return;
        };
        grant
            .expires_at
            .store(Some(Instant::now() + self.recovery_window));
        self.remove_expired(Instant::now());
    }

    fn remove_expired(&self, now: Instant) {
        for peer_grants in self.grants.values() {
            for (grant_id, grant) in peer_grants.iter() {
                if !grant.is_live(now) {
                    peer_grants.remove(&grant_id);
                }
            }
        }
    }
}

/// Cloneable capability used by replay/retry senders outside the coordinator.
/// Holding a grant proves that a head hint cannot be emitted before its rooted
/// selective-CAR authority is installed.
#[derive(Clone)]
pub struct HeadHintCarAuthority {
    access: Arc<SelectiveCarAccess>,
}

impl Default for HeadHintCarAuthority {
    fn default() -> Self {
        Self {
            access: Arc::new(SelectiveCarAccess::default()),
        }
    }
}

impl HeadHintCarAuthority {
    /// Bind the coordinator to its database's durable selected-transfer authority.
    /// The first binding owns this capability for its lifetime, including before
    /// the first send after reopening the node.
    pub fn bind_peerstore<S>(&self, store: storage::stores::Peerstore<S>)
    where
        S: storage::corekv::Store + 'static,
    {
        self.access.durable.get_or_init_racy(|| Arc::new(store));
    }

    pub(super) fn new(access: Arc<SelectiveCarAccess>) -> Self {
        Self { access }
    }

    pub fn register(&self, peer_id: PeerId, root_cid: Cid) -> Option<HeadHintCarGrant> {
        self.access
            .register(peer_id, root_cid)
            .map(HeadHintCarGrant)
    }
}

#[must_use = "the CAR grant must cover the corresponding PushLog attempt"]
pub struct HeadHintCarGrant(#[allow(dead_code)] SelectiveCarAccessGuard);

pub(super) struct SelectiveCarAccessGuard {
    access: Arc<SelectiveCarAccess>,
    peer_id: PeerId,
    grant_id: u64,
}

impl Drop for SelectiveCarAccessGuard {
    fn drop(&mut self) {
        self.access.finish_push(&self.peer_id, self.grant_id);
    }
}

#[cfg(test)]
mod tests;
