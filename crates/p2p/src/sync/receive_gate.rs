//! A check on pushed documents in the receive path, before their blocks are
//! fetched or stored (Go Shinzo's replication filter and retention gate,
//! #2456). The sync layer knows nothing of collections, so the node installs
//! a gate the database implements; the merge keeps its own checks as the
//! backstop. `proofs/tla/ReceiveGate.tla` models where the checks run.

use std::sync::Arc;

use async_trait::async_trait;
use bytes::Bytes;
use cid::Cid;
use rapidhash::RapidHashMap;

use crate::sync::manager::arrival_stats::SkipReason;

/// Why the gate refused a pushed document.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GateRefusal {
    /// The replication filter refused it.
    Filtered,
    /// Its height is at or below the retention floor.
    BelowFloor,
    /// It has no valid height for the retention floor.
    NoHeight,
}

/// Which check is asking: the one on arrival, the one on a fetched CAR, or
/// the one on a height block fetched alone before a block-by-block walk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GateStage {
    Arrival,
    AfterFetch,
    AfterHeightFetch,
}

impl GateRefusal {
    /// The skip a refusal at `stage` counts as, by Go's name.
    pub fn skip(self, stage: GateStage) -> SkipReason {
        match (self, stage) {
            (Self::Filtered, GateStage::Arrival) => SkipReason::Filtered,
            (Self::Filtered, GateStage::AfterFetch | GateStage::AfterHeightFetch) => {
                SkipReason::FilteredAfterFetch
            }
            (Self::BelowFloor, GateStage::Arrival) => SkipReason::RetentionBeforeFetch,
            (Self::BelowFloor, GateStage::AfterFetch) => SkipReason::RetentionAfterCarFetch,
            (Self::BelowFloor, GateStage::AfterHeightFetch) => {
                SkipReason::RetentionAfterHeightFetch
            }
            (Self::NoHeight, _) => SkipReason::RetentionNoHeight,
        }
    }
}

/// Where the gate reads the blocks a head links.
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait GateBlocks: defra_core::thread_bounds::MaybeSendSync {
    /// The block `cid`, if it is at hand, its bytes already checked against
    /// the CID.
    async fn block(&self, cid: &Cid) -> Option<Bytes>;
}

/// Decides whether a pushed document goes on.
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait ReceiveGate: defra_core::thread_bounds::MaybeSendSync {
    /// The refusal for the document `doc_id` of the collection
    /// `collection_id` whose head is `head`, or `None` to let it go on. A
    /// value the gate cannot read lets the document go on: the next check,
    /// or the merge, judges it.
    async fn judge(
        &self,
        collection_id: &str,
        doc_id: &str,
        head: &[u8],
        blocks: &dyn GateBlocks,
    ) -> Option<GateRefusal>;

    /// The block a create head links under the retention rule's height
    /// field (Go's `heightLink`), or `None` when the rule does not judge
    /// `head`. It is the one block fetched alone before a document is
    /// walked block by block.
    async fn height_link(&self, collection_id: &str, head: &[u8]) -> Option<Cid>;
}

/// Blocks held in memory, as a CAR carries them, in front of another
/// source.
pub struct CarBlocks<'a> {
    blocks: RapidHashMap<Cid, Bytes>,
    behind: Option<&'a dyn GateBlocks>,
}

impl<'a> CarBlocks<'a> {
    /// `blocks`, then whatever `behind` holds.
    pub fn new<I>(blocks: I, behind: Option<&'a dyn GateBlocks>) -> Self
    where
        I: IntoIterator<Item = (Cid, Bytes)>,
    {
        Self {
            blocks: blocks.into_iter().collect(),
            behind,
        }
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl GateBlocks for CarBlocks<'_> {
    async fn block(&self, cid: &Cid) -> Option<Bytes> {
        match self.blocks.get(cid) {
            Some(block) => Some(block.clone()),
            None => match self.behind {
                Some(behind) => behind.block(cid).await,
                None => None,
            },
        }
    }
}

/// The gate a node runs, if any.
pub type SharedReceiveGate = Arc<dyn ReceiveGate>;
