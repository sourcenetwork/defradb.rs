//! The coordinator's view of block merge state: whether blocks merged, and
//! the terminal outcomes that discharge a pending-DAG registration, all
//! answered by the sync manager.

use blockstore::Blockstore;
use cid::Cid;

use super::SyncCoordinator;
use crate::error::Result;
use crate::transport::P2PTransport;

impl<B: Blockstore + 'static, T: P2PTransport> SyncCoordinator<B, T> {
    /// Mark a block as merged.
    pub async fn mark_as_merged(&self, cid: &Cid) -> Result<()> {
        self.manager.mark_as_merged(cid).await
    }

    /// Mark multiple blocks as merged in a single transaction.
    pub async fn mark_batch_as_merged(&self, cids: &[Cid]) -> Result<()> {
        self.manager.mark_batch_as_merged(cids).await
    }

    /// Retire stale receiver state for a root whose merged bit was already
    /// durable before this event was observed.
    pub async fn reconcile_merged_pending(&self, cid: &Cid) -> Result<bool> {
        self.manager.reconcile_merged_pending(cid).await
    }

    /// Quarantine a terminally-rejected pending-DAG root (#1128).
    pub async fn quarantine_pending_dag(&self, root_cid: &Cid, reason: &str) {
        self.manager.quarantine_pending_dag(root_cid, reason).await
    }

    /// Check if a block is merged.
    pub async fn is_merged(&self, cid: &Cid) -> Result<bool> {
        self.manager.is_merged(cid).await
    }

    /// Get all unmerged block CIDs.
    pub async fn get_unmerged(&self) -> Result<Vec<Cid>> {
        self.manager.get_unmerged().await
    }
}
