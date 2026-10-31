//! Go Shinzo's height-only fetch (#2464). When a create's CAR does not come
//! back whole, the fetch is about to walk the document block by block. Before
//! it does, only the block the create links under the retention field is
//! fetched, and the receive gate judges it, so a document below the floor is
//! never walked. A height block several documents share is fetched once.
//! `proofs/tla/ReceiveGate.tla` models the step.

use blockstore::Blockstore;
use cid::Cid;

use super::receive_gate::StoredBlocks;
use crate::sync::manager::arrival_stats::SkipReason;
use crate::sync::queue::ProcessQueue;
use crate::sync::receive_gate::{CarBlocks, GateRefusal, GateStage, SharedReceiveGate};

/// The gate and the single-flight owners of the height blocks it fetches.
#[derive(Clone)]
pub(crate) struct HeightGate {
    pub(crate) gate: SharedReceiveGate,
    pub(crate) fetches: ProcessQueue,
}

impl std::fmt::Debug for HeightGate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HeightGate").finish_non_exhaustive()
    }
}

impl HeightGate {
    /// The height block to fetch before walking the document whose head is
    /// `head`, or `None` when the gate does not judge it by height.
    pub(crate) async fn height_link(&self, collection_id: &str, head: &[u8]) -> Option<Cid> {
        self.gate.height_link(collection_id, head).await
    }

    /// The skip for the document whose head is `head`, judged on the blocks
    /// this node stores, the fetched height block among them, or `None` to
    /// walk it. Only the retention floor refuses here, as in Go: the filter
    /// had its turn on arrival and on the CAR.
    pub(crate) async fn refusal<B>(
        &self,
        blockstore: &B,
        collection_id: &str,
        doc_id: &str,
        head: &[u8],
    ) -> Option<SkipReason>
    where
        B: Blockstore,
    {
        let stored = StoredBlocks(blockstore);
        let blocks = CarBlocks::new(std::iter::empty(), Some(&stored));
        match self
            .gate
            .judge(collection_id, doc_id, head, &blocks)
            .await?
        {
            GateRefusal::Filtered => None,
            refusal => Some(refusal.skip(GateStage::AfterHeightFetch)),
        }
    }
}
