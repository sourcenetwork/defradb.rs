//! Bitswap query tracking and block storage.

use std::sync::Arc;

use cid::Cid;
use kovan::Atom;
use kovan_map::HopscotchMap;
use kovan_queue::seg_queue::SegQueue;
use rapidhash::fast::RandomState;
use rapidhash::HashMapExt;

use blockstore::{verify_block_cid, Blockstore};

use crate::error::{Error, Result};
use crate::sync::manager::events::SyncEvent;
use crate::QueryId;

use super::SyncManager;

/// A completion sender parked in a queue so whoever resolves the query can
/// take ownership of it.
type CompletionSlot = Arc<SegQueue<tokio::sync::oneshot::Sender<FetchCompletion>>>;

fn completion_slot(sender: tokio::sync::oneshot::Sender<FetchCompletion>) -> CompletionSlot {
    let slot = SegQueue::new();
    slot.push(sender);
    Arc::new(slot)
}

/// Terminal observation for one transport fetch query.
///
/// `Deferred` is local receiver contention: the provider returned a useful
/// CAR, but another storage owner currently owns one of its CIDs. It releases
/// the fetch lease without consuming provider-failure attempts; the durable
/// per-root clock remains the only redrive owner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FetchCompletion {
    Success,
    Failure,
    Deferred,
    SizeLimit(Cid),
}

impl FetchCompletion {
    fn from_success(success: bool) -> Self {
        if success {
            Self::Success
        } else {
            Self::Failure
        }
    }
}

/// Completion signal for poll-owned exact-CID queries. Transport completion
/// already exists; this tracker lets the same fetch owner stop its blockstore
/// poll immediately when a provider failed instead of burning the full window.
///
/// A completion either finds its waiter or is latched for a waiter that has
/// not registered yet, and registration must see exactly one of the two, so
/// the state is replaced as one unit; it holds only in-flight queries.
#[derive(Debug, Clone, Default)]
pub(crate) struct BlockSyncCompletionTracker {
    state: Arc<Atom<BlockSyncCompletionState>>,
}

#[derive(Clone)]
struct BlockSyncCompletionState {
    waiters: rapidhash::RapidHashMap<QueryId, CompletionSlot>,
    early: rapidhash::RapidHashMap<QueryId, FetchCompletion>,
    early_order: std::collections::VecDeque<QueryId>,
    capacity: usize,
}

impl std::fmt::Debug for BlockSyncCompletionState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BlockSyncCompletionState")
            .field("waiters", &self.waiters.keys().collect::<Vec<_>>())
            .field("early", &self.early)
            .field("capacity", &self.capacity)
            .finish()
    }
}

impl Default for BlockSyncCompletionState {
    fn default() -> Self {
        Self::new(crate::sync::manager::config::DEFAULT_MAX_PENDING_DAGS)
    }
}

impl BlockSyncCompletionState {
    fn new(capacity: usize) -> Self {
        Self {
            waiters: rapidhash::RapidHashMap::new(),
            early: rapidhash::RapidHashMap::new(),
            early_order: std::collections::VecDeque::new(),
            capacity: capacity.max(1),
        }
    }

    fn remove_early(&mut self, query_id: QueryId) -> Option<FetchCompletion> {
        let result = self.early.remove(&query_id);
        if result.is_some() {
            self.early_order.retain(|candidate| *candidate != query_id);
        }
        result
    }

    /// Returns the unclaimed completion evicted to make room, if any.
    fn latch(&mut self, query_id: QueryId, completion: FetchCompletion) -> Option<QueryId> {
        if let std::collections::hash_map::Entry::Occupied(mut entry) = self.early.entry(query_id) {
            entry.insert(completion);
            return None;
        }
        let mut evicted = None;
        while self.early.len() >= self.capacity {
            let Some(oldest) = self.early_order.pop_front() else {
                break;
            };
            if self.early.remove(&oldest).is_some() {
                evicted = Some(oldest);
                break;
            }
        }
        self.early.insert(query_id, completion);
        self.early_order.push_back(query_id);
        evicted
    }
}

/// Completion signal for libp2p's two-stream rooted CAR protocol.  Request
/// dispatch and response arrival are separate streams, so blockstore polling
/// alone adds avoidable ownership latency at small admission capacities.
#[derive(Clone)]
pub(crate) struct RootedCarCompletionTracker {
    waiters: Arc<HopscotchMap<Cid, Arc<RootedCarWaiter>, RandomState>>,
}

impl Default for RootedCarCompletionTracker {
    fn default() -> Self {
        Self {
            waiters: Arc::new(HopscotchMap::with_hasher(RandomState::default())),
        }
    }
}

impl std::fmt::Debug for RootedCarCompletionTracker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RootedCarCompletionTracker")
            .field("waiters", &self.waiters.len())
            .finish()
    }
}

struct RootedCarWaiter {
    peer: crate::transport::PeerId,
    completion: SegQueue<tokio::sync::oneshot::Sender<FetchCompletion>>,
}

impl RootedCarCompletionTracker {
    pub(crate) fn register(
        &self,
        root_cid: Cid,
        peer_id: crate::transport::PeerId,
    ) -> tokio::sync::oneshot::Receiver<FetchCompletion> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        let completion = SegQueue::new();
        completion.push(tx);
        self.waiters.insert(
            root_cid,
            Arc::new(RootedCarWaiter {
                peer: peer_id,
                completion,
            }),
        );
        rx
    }

    pub(crate) fn complete(
        &self,
        root_cid: Cid,
        peer_id: &crate::transport::PeerId,
        success: bool,
    ) -> bool {
        self.complete_with(root_cid, peer_id, FetchCompletion::from_success(success))
    }

    pub(crate) fn defer(&self, root_cid: Cid, peer_id: &crate::transport::PeerId) -> bool {
        self.complete_with(root_cid, peer_id, FetchCompletion::Deferred)
    }

    pub(crate) fn complete_with(
        &self,
        root_cid: Cid,
        peer_id: &crate::transport::PeerId,
        completion: FetchCompletion,
    ) -> bool {
        let Some(expected) = self
            .waiters
            .get(&root_cid)
            .filter(|waiter| &waiter.peer == peer_id)
        else {
            return false;
        };
        let Some(waiter) = self.waiters.remove(&root_cid) else {
            return false;
        };
        if !Arc::ptr_eq(&waiter, &expected) {
            self.waiters.insert_if_absent(root_cid, waiter);
            return false;
        }
        if let Some(sender) = waiter.completion.pop() {
            let _ = sender.send(completion);
        }
        true
    }

    pub(crate) fn cancel(&self, root_cid: Cid) {
        if let Some(waiter) = self.waiters.remove(&root_cid) {
            drop(waiter.completion.pop());
        }
    }
}

impl BlockSyncCompletionTracker {
    pub(crate) fn with_capacity(capacity: usize) -> Self {
        Self {
            state: Arc::new(Atom::new(BlockSyncCompletionState::new(capacity))),
        }
    }

    /// Apply `f` to a copy of the state and publish it. `f` may run more than
    /// once under contention, so it must stay pure.
    fn update<R>(&self, mut f: impl FnMut(&mut BlockSyncCompletionState) -> R) -> R {
        loop {
            let current = self.state.load();
            let mut next = BlockSyncCompletionState::clone(&current);
            let result = f(&mut next);
            if self.state.compare_and_swap(&current, next).is_ok() {
                return result;
            }
        }
    }

    pub(crate) fn register(
        &self,
        query_id: QueryId,
    ) -> tokio::sync::oneshot::Receiver<FetchCompletion> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        let slot = completion_slot(tx);
        let early = self.update(|state| {
            let early = state.remove_early(query_id);
            if early.is_none() {
                state.waiters.insert(query_id, Arc::clone(&slot));
            }
            early
        });
        if let Some(completion) = early {
            if let Some(sender) = slot.pop() {
                let _ = sender.send(completion);
            }
        }
        rx
    }

    pub(crate) fn complete(&self, query_id: QueryId, success: bool) -> bool {
        self.complete_with(query_id, FetchCompletion::from_success(success))
    }

    pub(crate) fn defer(&self, query_id: QueryId) -> bool {
        self.complete_with(query_id, FetchCompletion::Deferred)
    }

    pub(crate) fn size_limit(&self, query_id: QueryId, cid: Cid) -> bool {
        self.complete_with(query_id, FetchCompletion::SizeLimit(cid))
    }

    fn complete_with(&self, query_id: QueryId, completion: FetchCompletion) -> bool {
        // Iroh allocates and dispatches the transport query before
        // sync_blocks returns its ID. A fast failure can therefore arrive
        // before the poll owner installs its waiter. Latch the terminal
        // result so registration observes state, not a lossy edge.
        let (waiter, evicted, capacity) =
            self.update(|state| match state.waiters.remove(&query_id) {
                Some(waiter) => (Some(waiter), None, state.capacity),
                None => (None, state.latch(query_id, completion), state.capacity),
            });
        if let Some(oldest) = evicted {
            tracing::warn!(
                query_id = oldest.0,
                capacity,
                "Evicting unclaimed block-sync completion at bounded capacity"
            );
        }
        let Some(waiter) = waiter else {
            return false;
        };
        if let Some(sender) = waiter.pop() {
            let _ = sender.send(completion);
        }
        true
    }

    pub(crate) fn cancel(&self, query_id: QueryId) {
        let waiter = self.update(|state| {
            state.remove_early(query_id);
            state.waiters.remove(&query_id)
        });
        if let Some(waiter) = waiter {
            drop(waiter.pop());
        }
    }

    pub(crate) fn take_early(&self, query_id: QueryId) -> Option<FetchCompletion> {
        self.update(|state| state.remove_early(query_id))
    }
}

impl<B: Blockstore + 'static> SyncManager<B> {
    pub(crate) fn block_sync_completion_tracker(&self) -> BlockSyncCompletionTracker {
        self.block_sync_completions.clone()
    }

    pub(crate) fn rooted_car_completion_tracker(&self) -> RootedCarCompletionTracker {
        self.rooted_car_completions.clone()
    }
    /// Register a Bitswap query for tracking.
    ///
    /// This maps the QueryId to the root CID so we can identify
    /// which DAG a completion event belongs to.
    pub(crate) fn register_query(
        &self,
        query_id: QueryId,
        root_cid: Cid,
    ) -> Option<FetchCompletion> {
        self.query_to_root.insert(query_id, root_cid);
        self.block_sync_completions.take_early(query_id)
    }

    /// Remove and return the root CID associated with a Bitswap query.
    pub fn take_query_root(&self, query_id: QueryId) -> Option<Cid> {
        self.query_to_root.remove(&query_id)
    }

    /// Handle Bitswap query completion.
    ///
    /// Called when a Bitswap sync completes (success or failure).
    pub async fn handle_bitswap_complete(
        &self,
        query_id: QueryId,
        success: bool,
        error: Option<String>,
    ) -> Result<()> {
        // Find the root CID for this query
        let root_cid = match self.query_to_root.remove(&query_id) {
            Some(cid) => cid,
            None => {
                tracing::debug!(
                    query_id = ?query_id,
                    "Bitswap complete for unknown query, ignoring"
                );
                return Ok(());
            }
        };

        if success {
            // All blocks fetched - emit BlockReceived for the root
            let dag = self
                .pending_dags
                .update(|pending| pending.remove(&root_cid));
            match dag {
                Some(dag) => {
                    tracing::info!(
                        cid = %root_cid,
                        doc_id = %dag.doc_id,
                        "Bitswap sync complete, emitting BlockReceived"
                    );

                    if self
                        .event_tx
                        .send(SyncEvent::BlockReceived {
                            cid: root_cid,
                            doc_id: dag.doc_id,
                            collection_id: dag.collection_id,
                            creator: dag.creator,
                            sender_peer: dag.source_peer,
                            is_explicit_replicator: dag.is_explicit_replicator,
                            explicit_replay_authorization: dag.explicit_replay_authorization,
                        })
                        .await
                        .is_err()
                    {
                        tracing::error!(
                            cid = %root_cid,
                            "Failed to send BlockReceived after Bitswap complete - receiver dropped"
                        );
                        return Err(Error::ChannelSend);
                    }
                }
                None => {
                    // This can happen if the DAG was processed by another path,
                    // cleaned up, or if there's a race condition
                    tracing::warn!(
                        cid = %root_cid,
                        "Bitswap sync completed but no pending DAG found - \
                         DAG may have been processed by another path or cleaned up"
                    );
                }
            }
        } else {
            // Sync failed - emit error, clean up
            self.pending_dags.update(|pending| {
                pending.remove(&root_cid);
            });

            let error_msg = error.unwrap_or_else(|| "Bitswap sync failed".to_string());
            tracing::warn!(
                cid = %root_cid,
                error = %error_msg,
                "Bitswap sync failed"
            );

            if self
                .event_tx
                .send(SyncEvent::SyncError {
                    cid: root_cid,
                    error: error_msg,
                })
                .await
                .is_err()
            {
                tracing::warn!(
                    cid = %root_cid,
                    "Failed to send SyncError event - receiver dropped"
                );
                return Err(Error::ChannelSend);
            }
        }

        Ok(())
    }

    /// Store a block received via Bitswap and check if pending DAGs can now proceed.
    ///
    /// This is called when blocks are fetched via Bitswap during DAG synchronization.
    /// The block is stored in the blockstore, and we check if any pending DAGs are
    /// now complete and can be processed.
    ///
    /// Returns `true` if the block was stored (not a duplicate).
    pub async fn store_bitswap_block(&self, cid: &Cid, data: &[u8]) -> Result<bool> {
        // PushLog, CAR, Bitswap and merge all mutate the same per-CID merge
        // marker. Keep one storage owner, but never retain the transport's
        // state-bearing block event while waiting for it: the current owner or
        // the receiver clock will re-check the pending frontier.
        let Some(_storage_owner) = self.process_queue.try_acquire_nowait(cid) else {
            self.diagnostics.record_single_flight_suppressed();
            tracing::debug!(
                cid = %cid,
                "Coalescing Bitswap block behind the current storage owner"
            );
            return Ok(false);
        };

        // Check if we already have the block
        if self
            .blockstore
            .has(cid)
            .await
            .map_err(|e| Error::BlockstoreError(e.to_string()))?
        {
            tracing::debug!(
                cid = %cid,
                "Bitswap block already in blockstore (duplicate)"
            );
            return Ok(false);
        }

        // Verify CID matches block content before storing (findings 06-29, 06-23, 06-24).
        if let Err(e) = verify_block_cid(cid, data) {
            let p2p_err = crate::error::blockstore_verify_to_p2p(e, cid);
            tracing::warn!(
                cid = %cid,
                error = %p2p_err,
                "Bitswap block failed CID verification, discarding"
            );
            return Err(p2p_err);
        }

        // Store the block
        if let Err(e) = self.blockstore.put(cid, data).await {
            tracing::error!(
                cid = %cid,
                error = %e,
                "Failed to store Bitswap block"
            );
            return Err(Error::BlockstoreError(e.to_string()));
        }

        tracing::info!(
            cid = %cid,
            data_len = data.len(),
            "Stored Bitswap block in blockstore"
        );

        for root_cid in self.pending_dags.read(|pending| pending.waiting_roots(cid)) {
            tracing::debug!(
                root_cid = %root_cid,
                received_cid = %cid,
                "Pending DAG received a missing block - will check completeness"
            );
        }

        Ok(true)
    }
}

#[cfg(test)]
mod completion_tracker_tests {
    use super::*;

    #[tokio::test]
    async fn poll_owner_observes_transport_completion_exactly_once() {
        let tracker = BlockSyncCompletionTracker::default();
        let query_id = QueryId(42);
        let receiver = tracker.register(query_id);

        assert!(tracker.complete(query_id, false));
        assert_eq!(
            receiver.await.expect("completion sender alive"),
            FetchCompletion::Failure
        );
        assert!(!tracker.complete(query_id, true));
    }

    #[tokio::test]
    async fn completion_before_waiter_registration_is_latched() {
        let tracker = BlockSyncCompletionTracker::with_capacity(1);
        let query_id = QueryId(44);

        assert!(!tracker.complete(query_id, false));
        let receiver = tracker.register(query_id);

        assert_eq!(
            receiver.await.expect("latched completion sender alive"),
            FetchCompletion::Failure
        );
        assert!(tracker.take_early(query_id).is_none());
    }

    #[tokio::test]
    async fn unclaimed_completion_latch_is_bounded() {
        let tracker = BlockSyncCompletionTracker::with_capacity(1);
        let first = QueryId(45);
        let second = QueryId(46);

        assert!(!tracker.complete(first, false));
        assert!(!tracker.complete(second, true));

        assert!(tracker.take_early(first).is_none());
        assert_eq!(tracker.take_early(second), Some(FetchCompletion::Success));
    }

    #[tokio::test]
    async fn cancelled_poll_owner_does_not_retain_a_completion_waiter() {
        let tracker = BlockSyncCompletionTracker::default();
        let query_id = QueryId(43);
        let receiver = tracker.register(query_id);
        tracker.cancel(query_id);

        assert!(receiver.await.is_err());
        assert!(!tracker.complete(query_id, false));
    }

    #[tokio::test]
    async fn contended_ingest_has_a_distinct_deferred_completion() {
        let tracker = BlockSyncCompletionTracker::default();
        let query_id = QueryId(47);
        let receiver = tracker.register(query_id);

        assert!(tracker.defer(query_id));
        assert_eq!(
            receiver.await.expect("completion sender alive"),
            FetchCompletion::Deferred
        );
    }
}
