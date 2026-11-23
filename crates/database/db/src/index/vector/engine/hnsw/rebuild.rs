//! Graph rebuild: the pass that reclaims what lazy maintenance cannot.

use super::Hnsw;
use crate::index::error::Result;
use crate::index::vector::store::VectorNodeStore;

/// Tombstones a graph can carry before a rebuild pays for itself, even on a
/// small one. Below this the waste is not worth a whole reinsert; above it the
/// ratio check decides.
const REBUILD_MIN_WASTE: u64 = 64;

impl<S: VectorNodeStore> Hnsw<S> {
    /// Whether the next rebuild is due.
    ///
    /// Two triggers, both O(1) reads of the meta record:
    ///
    /// - `rebuilds == 0`: the graph was last written by code before
    ///   self-link-free inserts, so it is owed one healing rebuild that
    ///   purges every stale self-link in a single pass (#1468). A graph
    ///   created by the current code stamps one at birth and never takes this
    ///   branch.
    /// - waste has reached a quarter of the live count: tombstones and
    ///   lazily-found stale links are costing search more than a rebuild
    ///   costs once.
    pub async fn should_rebuild(&self) -> Result<bool> {
        let Some(meta) = self.store.get_meta().await? else {
            return Ok(false);
        };
        Ok(meta.rebuilds == 0 || (meta.waste >= REBUILD_MIN_WASTE && meta.waste >= meta.live / 4))
    }

    /// Rebuilds the graph from the vectors it stores.
    ///
    /// Live nodes are snapshotted first, because the rebuild clears the build
    /// it is reading; every vector is then reinserted into a fresh graph.
    /// That purges what lazy maintenance cannot reach in one pass: stale
    /// self-links written before this graph's inserts refused them, links to
    /// tombstoned nodes, and the tombstoned nodes' own records. Counters are
    /// trued up by the reinserts, so a legacy meta whose decoded counters read
    /// zeroed is corrected here rather than drifting.
    ///
    /// A whole-graph rebuild is too heavy for a write's transaction, which is
    /// why the background task drives this rather than the write path.
    pub async fn rebuild(&mut self) -> Result<()> {
        let mut corpus = Vec::new();
        self.store
            .iterate_nodes(|node| {
                corpus.push((node.id, node.vector));
                Ok(())
            })
            .await?;
        self.store.clear().await?;
        for (id, vector) in &corpus {
            self.insert(*id, vector).await?;
        }
        Ok(())
    }
}
