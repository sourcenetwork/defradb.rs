use async_trait::async_trait;
use bytes::Bytes;
use cid::Cid;
use defra_core::merge::MergeBlock;
use defra_core::thread_bounds::MaybeSendSync;

/// A composite that merged on re-drive rather than on its own first attempt.
///
/// Its first attempt returned a deferred verdict, so the replication layer
/// never saw a merge for it and never marked it merged or fanned it out. This
/// carries what that post-merge path needs.
pub struct RedrivenMerge {
    pub cid: Cid,
    pub block_data: Bytes,
    pub doc_id: String,
    pub collection_id: String,
    /// The creator the carrier named, empty when it named none.
    pub creator: String,
}

impl RedrivenMerge {
    pub(crate) fn new(entry: &MergeBlock, block_data: Bytes) -> Self {
        Self {
            cid: entry.cid,
            block_data,
            doc_id: entry.doc_id.clone(),
            collection_id: entry.collection_id.clone(),
            creator: entry.creator.clone(),
        }
    }
}

/// Where a re-driven merge is reported, so it takes the same post-merge path
/// as a composite that merged on its first attempt.
///
/// Called once per composite that re-drive merged, whatever triggered the
/// re-drive: an inbound block, recovery, or a local write releasing a waiter.
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait RedrivenMergeSink: MaybeSendSync {
    async fn forward(&self, merged: RedrivenMerge);
}
