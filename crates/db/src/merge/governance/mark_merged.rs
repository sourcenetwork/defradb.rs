use std::sync::Arc;

use async_trait::async_trait;
use defra_core::thread_bounds::MaybeSendSync;

use super::redriven::{RedrivenMerge, RedrivenMergeSink};

/// [`RedrivenMergeSink`] for a node with no replication stack running: a
/// composite that merges on re-drive is marked merged and goes nowhere
/// else, since there is no peer to fan it out to. The sweep a node runs
/// while offline reports through this; the replication stack's own sink
/// takes over when one starts.
pub struct MarkMergedSink<B: blockstore::Blockstore> {
    blockstore: Arc<B>,
}

impl<B: blockstore::Blockstore> MarkMergedSink<B> {
    pub fn new(blockstore: Arc<B>) -> Self {
        Self { blockstore }
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl<B> RedrivenMergeSink for MarkMergedSink<B>
where
    B: blockstore::Blockstore + MaybeSendSync + 'static,
{
    async fn forward(&self, merged: RedrivenMerge) {
        if let Err(error) = self.blockstore.mark_as_merged(&merged.cid).await {
            tracing::warn!(cid = %merged.cid, %error, "Re-driven merge not marked merged");
        }
    }
}
