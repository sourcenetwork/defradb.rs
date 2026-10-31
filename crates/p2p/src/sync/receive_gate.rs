//! A check on pushed documents in the receive path, before their blocks are
//! fetched or stored. The sync layer knows nothing of collections, so the
//! node installs a gate the database implements (its replication filter);
//! the merge keeps its own check as the backstop.
//! `proofs/tla/ReceiveGate.tla` models where the checks run.

use std::sync::Arc;

use async_trait::async_trait;
use bytes::Bytes;
use cid::Cid;
use rapidhash::RapidHashMap;

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
    /// Whether the document `doc_id` of the collection `collection_id`,
    /// whose head is `head`, goes on. A value the gate cannot read lets the
    /// document go on: the next check, or the merge, judges it.
    async fn allows(
        &self,
        collection_id: &str,
        doc_id: &str,
        head: &[u8],
        blocks: &dyn GateBlocks,
    ) -> bool;
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
