//! Durable pending-DAG registrations (#1099), and the node's view of them.
//!
//! A hub success-acks a PushLog whose DAG has missing links on the strength of
//! its pending-DAG registration; the pusher then deletes its persisted retry
//! record. If the registration lived only in process memory, a crash between
//! the ack and Bitswap completion would silently lose the document (modeled in
//! `proofs/tla/PendingDagRestart.tla`). Each push-originated registration is
//! therefore durable: written before the success reply, deleted only when the
//! root is merged or quarantined (#1128, a deterministic content rejection
//! that will never succeed on replay; never at DagReady emission, TTL
//! eviction, or clear), and re-driven at startup and by the periodic resync
//! sweep.
//!
//! Every transition is one engine transaction of the [`PendingDagStorage`]:
//! registering a root, with the retirement of its sender scope's older root
//! and a slot under the durable cap ([`slots`]); discharging it after a merge;
//! quarantining it; and the sweep that reads every record. Concurrent
//! transitions that touch the same keys settle in the engine, so no lock
//! orders them. Each one updates the [`PersistedView`], the in-memory view of
//! what is durable, from its commit, in commit-receipt order: the view sees
//! the transitions of one root in the order they committed.
//!
//! Pull-originated registrations (DocSync/BranchableSync) are not persisted:
//! no remote retry state is destroyed by their acks, so restart loss is
//! recoverable by re-issuing the pull.

mod pooled;
mod slots;
mod store;
#[cfg(test)]
mod tests;
mod view;

use async_trait::async_trait;
use cid::Cid;
use defra_core::thread_bounds::MaybeSendSync;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use web_time::{SystemTime, UNIX_EPOCH};

use crate::error::{Error, Result};
use crate::ExplicitReplayAuthorization;

pub use pooled::pooled_pending_store;
pub(crate) use store::footprint as record_footprint;
pub use store::PendingDagStore;
pub use view::PersistedView;

/// Maximum number of same-root announcers retained in addition to the
/// immutable origin provider. Each unavailable provider consumes a bounded
/// selective-CAR attempt, so this durable list must remain bounded too.
pub(crate) const MAX_PENDING_DAG_ALTERNATE_PROVIDERS: usize = 3;

/// Verified explicit-replay authorization carried by a persisted registration.
/// The original proof is retained so a restored replay can authenticate a
/// reverse KMS request without trusting the request's claimed DID.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PersistedReplayAuthorization {
    pub source_peer_id: String,
    pub target_peer_id: String,
    pub collection_id: String,
    pub authorizer_did: String,
    pub expires_at: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capability: Option<String>,
}

impl From<&ExplicitReplayAuthorization> for PersistedReplayAuthorization {
    fn from(auth: &ExplicitReplayAuthorization) -> Self {
        Self {
            source_peer_id: auth.source_peer_id.clone(),
            target_peer_id: auth.target_peer_id.clone(),
            collection_id: auth.collection_id.clone(),
            authorizer_did: auth.authorizer_did.clone(),
            expires_at: auth.expires_at,
            capability: auth.capability.clone(),
        }
    }
}

impl From<PersistedReplayAuthorization> for ExplicitReplayAuthorization {
    fn from(auth: PersistedReplayAuthorization) -> Self {
        Self {
            source_peer_id: auth.source_peer_id,
            target_peer_id: auth.target_peer_id,
            collection_id: auth.collection_id,
            authorizer_did: auth.authorizer_did,
            expires_at: auth.expires_at,
            capability: auth.capability,
        }
    }
}

/// Compact durable form of one pending-DAG registration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PersistedPendingDag {
    pub doc_id: String,
    pub collection_id: String,
    #[serde(default)]
    pub head_priority: Option<u64>,
    pub creator: String,
    pub source_peer: Option<String>,
    /// Additional authenticated peers that announced this exact root.
    /// The original source remains immutable; alternates only extend the
    /// recovery rotation and therefore cannot hijack ownership.
    #[serde(default)]
    pub alternate_providers: Vec<String>,
    pub is_explicit_replicator: bool,
    pub explicit_replay_authorization: Option<PersistedReplayAuthorization>,
}

impl PersistedPendingDag {
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        defra_core::cbor::to_vec(self).map_err(|e| Error::Storage(e.to_string()))
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        defra_core::cbor::from_slice(bytes).map_err(|e| Error::Storage(e.to_string()))
    }
}

/// A pending-DAG record whose merge failed deterministically (e.g. a unique-index
/// rejection): moved out of the live keyspace, retained for forensics and counted,
/// and never re-driven by the resync sweep.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PersistedQuarantinedDag {
    pub record: PersistedPendingDag,
    pub reason: String,
    pub quarantined_at_unix_secs: u64,
}

impl PersistedQuarantinedDag {
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        defra_core::cbor::to_vec(self).map_err(|e| Error::Storage(e.to_string()))
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        defra_core::cbor::from_slice(bytes).map_err(|e| Error::Storage(e.to_string()))
    }

    /// Current wall-clock time as Unix seconds, for stamping `quarantined_at_unix_secs`.
    pub fn now_unix_secs() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    }
}

/// A registration to make durable.
#[derive(Debug, Clone, Copy)]
pub struct Registration<'a> {
    pub root: Cid,
    pub record: &'a PersistedPendingDag,
    /// Durable registrations allowed at once: the number of slots. `None`
    /// takes no slot: a deferred gossip head, bounded by bytes instead.
    pub capacity: Option<usize>,
}

/// How a registration ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Admission {
    /// The root holds a record: a new one, one that took over the slot of the
    /// older root of its sender scope it retired, or its own rewritten.
    Registered,
    /// A newer or equal head of the root's sender scope is durable already:
    /// nothing was written, and that head carries this one forward.
    Covered,
    /// Every slot is held: nothing was written.
    AtCapacity,
}

/// The durable registrations. Each method that changes them is one engine
/// transaction, and tells `view` what it changed, in commit-receipt order,
/// once it commits.
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait PendingDagStorage: MaybeSendSync {
    /// Registers a root: its record, the retirement of the older durable
    /// root of its sender scope, and, for a root new to the store that
    /// retires none, a free slot under the capacity.
    async fn register(
        &self,
        registration: Registration<'_>,
        view: &Arc<PersistedView>,
    ) -> Result<Admission>;

    /// Deletes a root's record and frees its slot. Returns whether it had a
    /// record.
    async fn remove(&self, root_cid: &Cid, view: &Arc<PersistedView>) -> Result<bool>;

    /// Moves a terminally rejected root to the quarantine keyspace: its
    /// quarantine record, and the deletion of its live record and slot.
    /// Returns whether it was quarantined before.
    async fn quarantine(
        &self,
        root_cid: &Cid,
        entry: &PersistedQuarantinedDag,
        view: &Arc<PersistedView>,
    ) -> Result<bool>;

    /// Every live record, read by a transaction that `view` is rebuilt from:
    /// a record that comes or goes while the sweep runs makes it run again.
    async fn sweep(&self, view: &Arc<PersistedView>) -> Result<Vec<(Cid, PersistedPendingDag)>>;

    /// Writes a record as it is, holding no slot and telling no view: for
    /// seeding a store, never for registering a root.
    async fn put(&self, root_cid: &Cid, record: &PersistedPendingDag) -> Result<()>;

    /// Every live record, as of one snapshot.
    async fn load_all(&self) -> Result<Vec<(Cid, PersistedPendingDag)>>;

    async fn is_quarantined(&self, root_cid: &Cid) -> Result<bool>;
    async fn load_quarantined(&self) -> Result<Vec<(Cid, PersistedQuarantinedDag)>>;
    async fn remove_quarantined(&self, root_cid: &Cid) -> Result<()>;
}
