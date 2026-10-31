//! Sync manager for coordinating P2P block synchronization.
//!
//! The SyncManager handles:
//! - Processing incoming PushLog messages
//! - Storing blocks in the blockstore with merge tracking
//! - Emitting events for database-layer CRDT merging
//! - Broadcasting local changes to the network
//!
//! # Architecture Note
//!
//! The P2P layer handles block storage and network coordination.
//! The actual CRDT merge is performed by the database layer.
//! This matches Go's architecture where p2p calls db.Merge().

mod bitswap;
mod join;
mod pending_dag;
mod pushlog;

pub(crate) use bitswap::{BlockSyncCompletionTracker, FetchCompletion, RootedCarCompletionTracker};
pub(crate) use pending_dag::PendingDagLease;

use crate::QueryId;
use cid::Cid;
use kovan_map::HopscotchMap;
use rapidhash::fast::RandomState;
use std::sync::Arc;
use tokio::sync::mpsc;

use blockstore::Blockstore;

use crate::sync::PeerStateTracker;

use super::super::queue::ProcessQueue;
use super::config::SyncConfig;
use super::diagnostics::SyncDiagnostics;
use super::events::SyncEvent;
use super::pending::SharedPendingDags;
use crate::sync::head_scope::ScopeDecision;
use crate::sync::pending_store::{PendingDagStorage, PersistedView};

/// Manager for P2P block synchronization.
///
/// # Usage
///
/// ```ignore
/// use p2p::sync::{SyncManager, SyncConfig};
/// use blockstore::DefraBlockstore;
///
/// // Create blockstore in P2P mode (merge tracking enabled)
/// let blockstore = DefraBlockstore::new(store, true);
///
/// // Create peer state tracker and sync manager
/// let peer_state = Arc::new(PeerStateTracker::new());
/// let (manager, mut events) = SyncManager::new(blockstore, peer_state, SyncConfig::default());
///
/// // Handle incoming PushLog
/// manager.process_pushlog(pushlog).await?;
///
/// // Process events
/// while let Some(event) = events.recv().await {
///     match event {
///         SyncEvent::BlockReceived { cid, doc_id, .. } => {
///             // Do CRDT merge
///             db.merge(&cid).await?;
///         }
///         _ => {}
///     }
/// }
/// ```
pub struct SyncManager<B: Blockstore> {
    /// Blockstore for storing/retrieving blocks
    pub(super) blockstore: Arc<B>,

    /// Single-flight owners of each CID's fetch and ingest.
    pub(super) process_queue: ProcessQueue,

    /// Channel for emitting sync events
    pub(super) event_tx: mpsc::Sender<SyncEvent>,

    /// Peer state tracker for finding providers
    pub(super) peer_state: Arc<PeerStateTracker>,

    /// Pending DAGs waiting for Bitswap to complete, indexed by root and each
    /// missing CID.
    pub(super) pending_dags: Arc<SharedPendingDags>,

    /// Coalesced wakeup for the sole bounded receiver dispatch owner.
    pub(super) pending_dag_ready: regolith::sync::Notify,

    /// Maps Bitswap QueryId → root CID for tracking completions.
    pub(super) query_to_root: HopscotchMap<QueryId, Cid, RandomState>,

    /// Completion waiters for poll-owned exact-CID queries.
    pub(super) block_sync_completions: BlockSyncCompletionTracker,

    /// Completion waiters for libp2p's separately streamed rooted CAR replies.
    pub(super) rooted_car_completions: RootedCarCompletionTracker,

    /// Observability counters (see `SyncDiagnostics`).
    pub(crate) diagnostics: Arc<SyncDiagnostics>,

    /// The node's check on pushed documents before their blocks are
    /// fetched or stored, if it has one.
    pub(crate) receive_gate: regolith::sync::OnceCell<crate::sync::receive_gate::SharedReceiveGate>,

    /// Single-flight owners of the height blocks fetched alone for the gate,
    /// so a height block several documents share is asked for once.
    pub(crate) height_fetches: ProcessQueue,

    /// Capacity of `pending_dags`; overflow is rejected with a backpressure nack.
    pub(super) max_pending_dags: usize,

    /// Durable backing for push-originated pending registrations (#1099).
    /// Empty until the embedding layer installs a store; process-local
    /// semantics apply while empty.
    pub(super) pending_store: regolith::sync::OnceCell<Arc<dyn PendingDagStorage>>,

    /// What is durable, as the store's commits say, in their receipt order:
    /// the roots with a record and each sender scope's current head.
    pub(super) persisted: Arc<PersistedView>,

    /// Single-flight guard for the durable resync sweep.
    pub(super) pending_resync_in_flight: std::sync::atomic::AtomicBool,

    /// Resync invocation counter; every Nth sweep skips the steady-state
    /// early-exit so store/set desyncs from rare races self-heal.
    pub(super) pending_resync_tick: std::sync::atomic::AtomicUsize,
}

/// Every Nth resync is a forced full sweep (see `pending_resync_tick`).
pub(super) const PENDING_RESYNC_FORCED_TICK: usize = 10;

/// Durable registrations may outlive their in-memory pending entries (TTL
/// eviction frees the map slot but not the recovery obligation), so the
/// durable set gets its own, larger cap. At the cap new registrations are
/// nacked, the hub refuses the obligation while the pusher still owns its
/// retry state, rather than accepting and later dropping it.
pub(super) const PERSISTED_PENDING_CAP_FACTOR: usize = 4;

impl<B: Blockstore + 'static> SyncManager<B> {
    /// True when a newer head for the same sender scope is already durable
    /// or pending, so this root's work is subsumed rather than dropped.
    ///
    /// Retiring a superseded root is deliberate: the newer head carries the
    /// same document forward, and driving both would duplicate the fetch.
    pub(super) fn scope_head_is_covered_by_current(
        &self,
        root_cid: Cid,
        source_peer: Option<&str>,
        collection_id: &str,
        doc_id: &str,
        head_priority: Option<u64>,
    ) -> bool {
        matches!(
            self.persisted.scope_decision(
                root_cid,
                source_peer,
                collection_id,
                doc_id,
                head_priority
            ),
            ScopeDecision::CoveredByCurrent
        ) || self.pending_dags.read(|pending| {
            pending.scope_head_is_covered_by_current(
                root_cid,
                source_peer,
                collection_id,
                doc_id,
                head_priority,
            )
        })
    }

    pub(super) fn scope_head_is_refresh_or_newer(
        &self,
        root_cid: Cid,
        source_peer: Option<&str>,
        collection_id: &str,
        doc_id: &str,
        head_priority: Option<u64>,
    ) -> bool {
        self.persisted.refreshes_or_supersedes(
            root_cid,
            source_peer,
            collection_id,
            doc_id,
            head_priority,
        ) || self.pending_dags.read(|pending| {
            pending.scope_head_is_refresh_or_newer(
                root_cid,
                source_peer,
                collection_id,
                doc_id,
                head_priority,
            )
        })
    }

    /// Create a new SyncManager.
    ///
    /// # Arguments
    ///
    /// * `blockstore` - The blockstore for storing/retrieving blocks
    /// * `peer_state` - Peer state tracker for finding block providers
    /// * `config` - Configuration options
    ///
    /// Returns the manager and a receiver for sync events.
    pub fn new(
        blockstore: Arc<B>,
        peer_state: Arc<PeerStateTracker>,
        config: SyncConfig,
    ) -> (Self, mpsc::Receiver<SyncEvent>) {
        let (event_tx, event_rx) = mpsc::channel(config.event_buffer_size);

        let manager = Self {
            blockstore,
            process_queue: ProcessQueue::new(),
            event_tx,
            peer_state,
            pending_dags: Arc::new(SharedPendingDags::default()),
            pending_dag_ready: regolith::sync::Notify::new(),
            query_to_root: HopscotchMap::with_hasher(RandomState::default()),
            block_sync_completions: BlockSyncCompletionTracker::with_capacity(
                config.max_pending_dags.max(1),
            ),
            rooted_car_completions: RootedCarCompletionTracker::default(),
            diagnostics: Arc::new(SyncDiagnostics::default()),
            receive_gate: regolith::sync::OnceCell::new(),
            height_fetches: ProcessQueue::new(),
            // A zero cap would reject every missing-link push forever
            // (permanent admission outage); normalize to a 1-slot map.
            max_pending_dags: config.max_pending_dags.max(1),
            pending_store: regolith::sync::OnceCell::new(),
            persisted: Arc::new(PersistedView::default()),
            pending_resync_in_flight: std::sync::atomic::AtomicBool::new(false),
            pending_resync_tick: std::sync::atomic::AtomicUsize::new(0),
        };

        (manager, event_rx)
    }

    /// Check if a block exists and is merged.
    pub async fn is_merged(&self, cid: &Cid) -> crate::error::Result<bool> {
        self.blockstore
            .is_merged(cid)
            .await
            .map_err(|e| crate::error::Error::BlockstoreError(e.to_string()))
    }

    /// Mark a block as merged (called by database layer after CRDT merge).
    ///
    /// A successful terminal merge is the point where a durable pending
    /// registration has discharged its recovery obligation (#1099): only
    /// here or by quarantine (#1128, `quarantine_pending_dag`, a
    /// deterministic content rejection that will never succeed on replay),
    /// and never at DagReady emission, TTL eviction, or clear, is the
    /// persisted record deleted.
    /// The gate a pushed document's fetch judges its height block with,
    /// when the node runs one.
    pub(crate) fn height_gate(&self) -> Option<crate::sync::coordinator::height_gate::HeightGate> {
        self.receive_gate
            .get()
            .map(|gate| crate::sync::coordinator::height_gate::HeightGate {
                gate: gate.clone(),
                fetches: self.height_fetches.clone(),
            })
    }

    pub async fn mark_as_merged(&self, cid: &Cid) -> crate::error::Result<()> {
        self.blockstore
            .mark_as_merged(cid)
            .await
            .map_err(|e| crate::error::Error::BlockstoreError(e.to_string()))?;
        self.discharge_merged(cid).await;
        Ok(())
    }

    /// Mark multiple blocks as merged in a single transaction.
    pub async fn mark_batch_as_merged(&self, cids: &[Cid]) -> crate::error::Result<()> {
        self.blockstore
            .mark_batch_as_merged(cids)
            .await
            .map_err(|e| crate::error::Error::BlockstoreError(e.to_string()))?;
        for cid in cids {
            self.discharge_merged(cid).await;
        }
        Ok(())
    }

    /// Reconcile an already-merged root through the same terminal cleanup
    /// path used by a newly completed merge.
    ///
    /// This closes the crash/retry seam where callers observe the durable
    /// merged bit but a stale live or persisted receiver obligation remains.
    pub async fn reconcile_merged_pending(&self, cid: &Cid) -> crate::error::Result<bool> {
        if !self.is_merged(cid).await? {
            return Ok(false);
        }
        self.discharge_merged(cid).await;
        Ok(true)
    }

    /// Sole successful-merge cleanup transition; the caller has established
    /// the durable merged bit. A root with a durable record has a pending
    /// entry from its registration until this cleanup or a TTL eviction, or
    /// is in the view, so this pays a transaction only for such a root; one
    /// missed after an eviction is discharged by the resync sweep, which
    /// finds it merged.
    pub(super) async fn discharge_merged(&self, cid: &Cid) {
        let had_entry = self.clear_pending_dag(cid);
        if (had_entry || self.persisted.contains(cid)) && self.remove_persisted_pending(cid).await {
            self.diagnostics.record_pending_dag_terminal_merged();
        }
    }

    /// The single-flight owners of each CID's fetch and ingest, for tests that
    /// hold a CID's owner.
    #[cfg(test)]
    pub(crate) fn process_queue(&self) -> ProcessQueue {
        self.process_queue.clone()
    }

    /// Try to acquire the shared ingest/merge owners for a rooted block batch.
    ///
    /// The root is included even when an exact selective-CAR response only
    /// carries descendants. This keeps response storage ordered with the
    /// root's PushLog registration and eventual merge, matching Go's
    /// root-owned ingest path. Each contained CID is included as well because
    /// Rust's SSI store conflict-checks the mutable `ToMergeIndexKey`; shared
    /// child blocks from different roots must therefore also have one writer.
    pub(crate) fn try_acquire_car_storage_owners(
        &self,
        root_cid: Cid,
        block_cids: Vec<Cid>,
    ) -> Result<Vec<crate::sync::queue::ProcessGuard>, Cid> {
        self.process_queue
            .try_acquire_all_nowait(std::iter::once(root_cid).chain(block_cids))
    }

    /// Get all unmerged block CIDs.
    pub async fn get_unmerged(&self) -> crate::error::Result<Vec<Cid>> {
        self.blockstore
            .get_unmerged()
            .await
            .map_err(|e| crate::error::Error::BlockstoreError(e.to_string()))
    }

    /// Get the blockstore reference.
    pub fn blockstore(&self) -> &Arc<B> {
        &self.blockstore
    }

    /// Get a clone of the sync event sender for spawning background tasks.
    pub fn event_sender(&self) -> mpsc::Sender<SyncEvent> {
        self.event_tx.clone()
    }

    /// Shared reference to the sync diagnostics counters.
    pub fn diagnostics(&self) -> Arc<SyncDiagnostics> {
        Arc::clone(&self.diagnostics)
    }

    /// Effective pending-DAG capacity.
    pub fn max_pending_dags(&self) -> usize {
        self.max_pending_dags
    }

    /// Number of durable pending-DAG registrations currently held.
    pub fn persisted_pending_count(&self) -> usize {
        self.persisted.len()
    }

    /// Cap on durable pending-DAG registrations; admission nacks beyond it.
    pub fn persisted_pending_capacity(&self) -> usize {
        self.max_pending_dags
            .saturating_mul(PERSISTED_PENDING_CAP_FACTOR)
    }

    /// Whether the durable resync sweep is currently running.
    pub fn pending_resync_in_flight(&self) -> bool {
        self.pending_resync_in_flight
            .load(std::sync::atomic::Ordering::Acquire)
    }

    /// Install the durable pending-DAG store. First-call-wins; later calls
    /// are discarded.
    ///
    /// Sweeps the store before returning, so the view of what is durable is
    /// whole from the first PushLog on.
    pub async fn install_pending_dag_store(&self, store: Arc<dyn PendingDagStorage>) {
        if self.pending_store.set(Arc::clone(&store)).is_err() {
            return;
        }
        match store.sweep(&self.persisted).await {
            Ok(_) => self
                .diagnostics
                .observe_persisted_pending_dag_depth(self.persisted.len()),
            Err(error) => {
                tracing::warn!(
                    error = %error,
                    "Failed to load persisted pending DAG roots; the first resync sweep loads them"
                );
            }
        }
        match store.load_quarantined().await {
            Ok(records) => self.persisted.set_quarantined(records.len()),
            Err(error) => {
                tracing::warn!(
                    error = %error,
                    "Failed to count quarantined pending DAGs; the gauge counts from 0"
                );
            }
        }
    }

    /// Number of pending-DAG roots currently quarantined (#1128).
    pub fn quarantined_pending_count(&self) -> usize {
        self.persisted.quarantined()
    }

    /// Quarantined roots this peer announced, from the durable store.
    pub async fn quarantined_from(
        &self,
        peer: &str,
    ) -> crate::error::Result<Vec<(Cid, crate::sync::pending_store::PersistedQuarantinedDag)>> {
        let Some(store) = self.pending_store() else {
            return Ok(Vec::new());
        };
        let mut records = store.load_quarantined().await?;
        records.retain(|(_, dag)| dag.record.source_peer.as_deref() == Some(peer));
        Ok(records)
    }

    pub(super) fn pending_store(&self) -> Option<Arc<dyn PendingDagStorage>> {
        self.pending_store.get().cloned()
    }

    /// Deletes a root's durable record; returns whether there was one. A
    /// failure is reported, and the resync sweep retries it.
    pub(super) async fn remove_persisted_pending(&self, root_cid: &Cid) -> bool {
        let Some(store) = self.pending_store() else {
            return false;
        };
        match store.remove(root_cid, &self.persisted).await {
            Ok(removed) => removed,
            Err(error) => {
                tracing::warn!(
                    root_cid = %root_cid,
                    error = %error,
                    "Failed to delete persisted pending DAG record"
                );
                false
            }
        }
    }
}
