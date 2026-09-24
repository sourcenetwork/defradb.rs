//! DAG sync state tracking.

use std::sync::atomic::{AtomicUsize, Ordering};

use cid::Cid;
use kovan_map::HopscotchMap;
use kovan_queue::seg_queue::SegQueue;
use rapidhash::fast::RandomState;
use tracing::debug;

/// Default maximum number of synced CIDs to track before eviction.
const DEFAULT_MAX_SYNCED_CIDS: usize = 100_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SyncPhase {
    Syncing,
    Synced,
}

/// Tracks ongoing DAG sync operations.
///
/// This is used to:
/// - Prevent duplicate sync requests for the same CID
/// - Track which blocks are being fetched
/// - Cancel sync operations when needed
///
/// The synced set has a configurable maximum size. When the limit is reached,
/// the oldest synced CIDs are evicted to make room for new ones. This prevents
/// unbounded memory growth in long-running nodes.
///
/// One map holds both phases, so claiming a CID is a single insert that fails
/// whether the CID is syncing or already synced.
pub struct DagSyncState {
    phases: HopscotchMap<Cid, SyncPhase, RandomState>,
    /// Order of synced CIDs for FIFO eviction (oldest first)
    synced_order: SegQueue<Cid>,
    synced_count: AtomicUsize,
    /// Maximum number of synced CIDs before eviction
    max_synced: usize,
}

impl Default for DagSyncState {
    fn default() -> Self {
        Self::new()
    }
}

impl DagSyncState {
    /// Create a new sync state tracker with default settings.
    pub fn new() -> Self {
        Self::with_max_synced(DEFAULT_MAX_SYNCED_CIDS)
    }

    /// Create a new sync state tracker with custom max synced limit.
    ///
    /// # Arguments
    ///
    /// * `max_synced` - Maximum number of synced CIDs to track. When exceeded,
    ///   the oldest synced CIDs are evicted.
    pub fn with_max_synced(max_synced: usize) -> Self {
        Self {
            phases: HopscotchMap::with_hasher(RandomState::default()),
            synced_order: SegQueue::new(),
            synced_count: AtomicUsize::new(0),
            max_synced,
        }
    }

    /// Check if a CID is currently being synced.
    pub async fn is_syncing(&self, cid: &Cid) -> bool {
        self.phases.get(cid) == Some(SyncPhase::Syncing)
    }

    /// Check if a CID has recently been synced.
    ///
    /// This is a bounded in-memory hint used to avoid duplicate work during a
    /// session, not a persistent claim. A `false` result may mean the CID was
    /// never synced or that it was evicted from the recent synced cache.
    pub async fn is_synced(&self, cid: &Cid) -> bool {
        self.phases.get(cid) == Some(SyncPhase::Synced)
    }

    /// Mark a CID as currently syncing.
    ///
    /// Returns false if already syncing or synced.
    /// This operation is atomic - no race condition between check and insert.
    pub async fn start_sync(&self, cid: Cid) -> bool {
        self.phases
            .insert_if_absent(cid, SyncPhase::Syncing)
            .is_none()
    }

    /// Mark a CID as successfully synced.
    ///
    /// If the synced set exceeds the maximum size, the oldest synced CIDs
    /// are evicted to make room.
    pub async fn complete_sync(&self, cid: Cid) {
        if self.phases.insert(cid, SyncPhase::Synced) == Some(SyncPhase::Synced) {
            return;
        }
        self.synced_order.push(cid);
        self.synced_count.fetch_add(1, Ordering::Relaxed);

        while self.synced_count.load(Ordering::Relaxed) > self.max_synced {
            let Some(old_cid) = self.synced_order.pop() else {
                break;
            };
            match self.phases.remove(&old_cid) {
                Some(SyncPhase::Synced) => {
                    let remaining = self.synced_count.fetch_sub(1, Ordering::Relaxed) - 1;
                    debug!(
                        cid = %old_cid,
                        synced_count = remaining,
                        max_synced = self.max_synced,
                        "Evicted old synced CID to stay within memory limit"
                    );
                }
                Some(SyncPhase::Syncing) => {
                    self.phases.insert_if_absent(old_cid, SyncPhase::Syncing);
                }
                None => {}
            }
        }
    }

    /// Cancel a sync operation (e.g., on error).
    pub async fn cancel_sync(&self, cid: &Cid) {
        if self.phases.remove(cid) == Some(SyncPhase::Synced) {
            self.phases.insert_if_absent(*cid, SyncPhase::Synced);
        }
    }

    /// Get all CIDs currently being synced.
    pub async fn syncing_cids(&self) -> Vec<Cid> {
        self.phases
            .iter()
            .filter(|(_, phase)| *phase == SyncPhase::Syncing)
            .map(|(cid, _)| cid)
            .collect()
    }

    /// Get the number of synced CIDs being tracked.
    pub async fn synced_count(&self) -> usize {
        self.synced_count.load(Ordering::Relaxed)
    }

    /// Clear all state (for testing or reset).
    pub async fn clear(&self) {
        self.phases.clear();
        while self.synced_order.pop().is_some() {}
        self.synced_count.store(0, Ordering::Relaxed);
    }
}
