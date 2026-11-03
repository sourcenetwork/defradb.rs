//! Sync manager events.

use cid::Cid;

use crate::ExplicitReplayAuthorization;

/// The channel the sync coordinator emits its [`SyncEvent`]s on.
pub type SyncEventReceiver = crate::rt::mpsc::Receiver<SyncEvent>;

/// A fetch permit of a root's source peer, taken by the retry clock together
/// with the root's fetch slot, so the fetch never holds a slot while it waits
/// for a permit. Only the fetch it was taken for uses it: a clone of the
/// event carries none.
#[derive(Debug, Default)]
pub struct DagFetchPeerPermit(Option<regolith::sync::OwnedSemaphorePermit>);

impl DagFetchPeerPermit {
    pub(crate) fn new(permit: regolith::sync::OwnedSemaphorePermit) -> Self {
        Self(Some(permit))
    }

    pub(crate) fn take(&mut self) -> Option<regolith::sync::OwnedSemaphorePermit> {
        self.0.take()
    }
}

impl Clone for DagFetchPeerPermit {
    fn clone(&self) -> Self {
        Self::default()
    }
}

/// Events emitted by the SyncManager for higher layers to process.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum SyncEvent {
    /// A new block was received and stored, needs CRDT merge.
    ///
    /// The database layer should process this by:
    /// 1. Loading the block from blockstore
    /// 2. Applying CRDT merge
    /// 3. Calling blockstore.mark_as_merged()
    BlockReceived {
        /// The CID of the received block
        cid: Cid,
        /// Document ID this block belongs to
        doc_id: String,
        /// Collection ID
        collection_id: String,
        /// Creator peer ID
        creator: String,
        /// The actual transport peer that sent this block to us.
        sender_peer: Option<String>,
        /// True when this block arrived via the explicit replicator push path.
        is_explicit_replicator: bool,
        /// Capability-based explicit replay authorization carried by two-stream pushes.
        explicit_replay_authorization: Option<ExplicitReplayAuthorization>,
    },

    /// Failed to process a sync request.
    SyncError { cid: Cid, error: String },

    /// DAG has missing blocks that need to be fetched.
    ///
    /// The coordinator should:
    /// 1. Prefer a source-peer DAG fetch when the announcing peer is known
    /// 2. Fall back to transport block sync when only provider hints are available
    DagNeedsFetch {
        /// Root CID of the DAG being synced
        root_cid: Cid,
        /// CIDs of missing blocks to fetch
        missing: Vec<Cid>,
        /// Suggested providers (peers that may have the blocks)
        providers: Vec<String>,
        /// Document ID for the root block
        doc_id: String,
        /// Collection ID for the root block
        collection_id: String,
        /// Creator of the root block
        creator: String,
        /// The actual transport peer that sent the root block to us.
        sender_peer: Option<String>,
        /// True when the root block arrived via the explicit replicator push path.
        is_explicit_replicator: bool,
        /// Capability-based explicit replay authorization carried by two-stream pushes.
        explicit_replay_authorization: Option<ExplicitReplayAuthorization>,
        /// The `sender_peer` fetch permit the retry clock took with this root's slot.
        peer_permit: DagFetchPeerPermit,
    },

    /// DAG is ready for merge after Bitswap fetch completed.
    ///
    /// All missing blocks have been fetched. The database layer should now
    /// process the complete DAG for CRDT merge.
    DagReady {
        /// Root CID of the completed DAG
        root_cid: Cid,
        /// Document ID
        doc_id: String,
        /// Collection ID
        collection_id: String,
        /// Creator of the root block.
        creator: String,
        /// The actual transport peer that sent the root block to us.
        sender_peer: Option<String>,
        /// True when the root block arrived via the explicit replicator push path.
        is_explicit_replicator: bool,
        /// Capability-based explicit replay authorization carried by two-stream pushes.
        explicit_replay_authorization: Option<ExplicitReplayAuthorization>,
    },
}
