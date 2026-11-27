//! The deferred maintenance half of [`VectorIndex`]: its pending writes, and
//! applying them (`crate::index::vector::pending`).

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use storage::corekv::{MaybeSend, Reader, Writer};

use super::{VectorIndex, LIVE_EPOCH};
use crate::index::error::Result;
use crate::index::vector::engine::ann::VectorIndexEngine;
use crate::index::vector::pending::{self, Backlog, Backlogs, PendingEntry, PendingOp};
use crate::index::vector::store::{KvNodeStore, NodeId};

impl VectorIndex {
    /// Lets a deferred index defer its writes, counting them in `backlogs`.
    /// Any other index is unchanged.
    pub fn defer_through(&mut self, backlogs: &Backlogs) {
        if self.is_deferred() {
            self.backlog = Some(backlogs.get(self.collection_short_id, self.desc.id));
        }
    }

    /// Whether the index is declared `maintenance: DEFERRED`.
    pub fn is_deferred(&self) -> bool {
        self.vector.maintenance.is_deferred()
    }

    /// This node's count of the index's pending writes, when it defers them.
    pub fn backlog(&self) -> Option<&Arc<Backlog>> {
        self.backlog.as_ref()
    }

    /// The pending writes made in this attempt; the counter stays shared, so
    /// a commit callback reads the final number.
    pub fn owed(&self) -> &Arc<AtomicU64> {
        &self.owed
    }

    /// Whether this write goes to the pending set: a deferred index with a
    /// drain behind it, not yet holding [`pending::PENDING_LIMIT`] writes.
    pub(super) fn defers_now(&self) -> bool {
        self.backlog
            .as_ref()
            .is_some_and(|backlog| !backlog.over_limit())
    }

    pub(super) fn store<'txn, T>(&self, txn: &'txn mut T) -> KvNodeStore<'txn, T> {
        KvNodeStore::new(txn, self.collection_short_id, self.desc.id, LIVE_EPOCH)
    }

    /// Records a pending write of `entry`'s node.
    pub(super) async fn defer<T>(&self, txn: &mut T, entry: PendingEntry) -> Result<()>
    where
        T: Reader + Writer + MaybeSend,
    {
        pending::put_pending(&mut self.store(txn), &entry).await?;
        self.owed.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    /// An inline write to a deferred index supersedes any pending write of
    /// the node, which the drain would otherwise apply over it later.
    pub(super) async fn supersede_pending<T>(&self, txn: &mut T, id: NodeId) -> Result<()>
    where
        T: Reader + Writer + MaybeSend,
    {
        if !self.is_deferred() {
            return Ok(());
        }
        pending::delete_pending(&mut self.store(txn), id).await
    }

    /// Every pending write of a deferred index, read on the caller's snapshot.
    pub(super) async fn pending_entries<T>(&self, txn: &mut T) -> Result<Vec<PendingEntry>>
    where
        T: Reader + Writer + MaybeSend,
    {
        if !self.is_deferred() {
            return Ok(Vec::new());
        }
        pending::list_pending(&self.store(txn), usize::MAX).await
    }

    /// Applies the planned nodes' pending writes and deletes them, returning
    /// how many there were. Each entry is read so that a write replacing it
    /// before this transaction commits fails the commit: an entry is never
    /// deleted unapplied. A planned node with no entry left is skipped.
    pub async fn apply_planned<T>(&self, txn: &mut T, planned: &[NodeId]) -> Result<usize>
    where
        T: Reader + Writer + MaybeSend,
    {
        let mut entries = Vec::with_capacity(planned.len());
        {
            let store = self.store(txn);
            for &id in planned {
                if let Some(entry) = pending::get_pending(&store, id).await? {
                    entries.push(entry);
                }
            }
        }
        self.apply_pending(txn, &entries).await?;
        let mut store = self.store(txn);
        for entry in &entries {
            pending::delete_pending(&mut store, entry.id).await?;
        }
        Ok(entries.len())
    }

    /// Applies pending writes to the graph on `txn`, as their writes would
    /// have in sync mode, then lets the engine train or build if due.
    pub async fn apply_pending<T>(&self, txn: &mut T, entries: &[PendingEntry]) -> Result<()>
    where
        T: Reader + Writer + MaybeSend,
    {
        let mut engine = self.engine(txn)?;
        for entry in entries {
            match entry.op {
                PendingOp::Upsert => engine.insert(entry.id, &entry.vector).await?,
                PendingOp::Delete => {
                    engine.delete(entry.id).await?;
                }
            }
        }
        if engine.should_build_inline().await? {
            engine.build().await?;
        }
        Ok(())
    }

    /// Applies every pending write of a deferred index, read as a drain
    /// reads them. Nothing for any other index.
    pub(super) async fn apply_all_pending<T>(&self, txn: &mut T) -> Result<usize>
    where
        T: Reader + Writer + MaybeSend,
    {
        if !self.is_deferred() {
            return Ok(0);
        }
        let pending = pending::list_pending(&self.store(txn), usize::MAX).await?;
        let ids: Vec<NodeId> = pending.iter().map(|entry| entry.id).collect();
        self.apply_planned(txn, &ids).await
    }
}
