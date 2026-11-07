use std::sync::Arc;

use regolith::sync::OnceCell;
use storage::corekv::Store;

use super::deferred::DeferredMerges;
use super::redriven::RedrivenMergeSink;
use crate::merge::merge_handler::{cid_set, CidSet, DbMergeHandler};

/// What the merge handler keeps for governed collections, shared by every
/// clone of the handler.
#[derive(Clone)]
pub(crate) struct GovernanceState {
    /// Composites a merge validator deferred, by the CIDs they await.
    pub(crate) deferred: Arc<DeferredMerges>,
    /// Composites a merge validator rejected. A reject rests on present bytes
    /// and never changes, and the block stays in the blockstore's unmerged
    /// set, so without this the governance sweep would re-judge every
    /// rejected composite each tick and, past its budget, never reach a
    /// deferred one. In memory: after a restart each is re-judged once.
    pub(crate) rejected: Arc<CidSet>,
    /// Where composites merged by re-drive are reported, so they take the
    /// same post-merge path as a first-attempt merge.
    redriven_sink: Arc<OnceCell<Arc<dyn RedrivenMergeSink>>>,
}

impl Default for GovernanceState {
    fn default() -> Self {
        Self {
            deferred: Arc::default(),
            rejected: Arc::new(cid_set()),
            redriven_sink: Arc::new(OnceCell::new()),
        }
    }
}

impl<S: Store, B: blockstore::Blockstore> DbMergeHandler<S, B> {
    /// Lower the deferred index's capacity, so a test can reach the
    /// at-capacity path without indexing
    /// [`super::MAX_DEFERRED_COMPOSITES`] composites first. Nothing in
    /// production calls this.
    pub fn set_deferred_capacity(&self, capacity: usize) {
        self.governance.deferred.set_capacity(capacity);
    }

    /// Report composites merged by re-drive to `sink`, which marks them merged
    /// and fans them out as the replication layer does a first-attempt merge.
    pub fn set_redriven_merge_sink(&self, sink: Arc<dyn RedrivenMergeSink>) {
        let _ = self.governance.redriven_sink.set(sink);
    }

    pub(crate) fn redriven_merge_sink(&self) -> Option<&Arc<dyn RedrivenMergeSink>> {
        self.governance.redriven_sink.get()
    }
}
