//! `RedrivenMergeSink` backed by the `SyncCoordinator`.
//!
//! A composite merged by re-drive never reached the replication layer as a
//! merge: its own delivery returned a deferred verdict. This gives it the
//! post-merge path `handle_replicated_block` gives a first-attempt merge —
//! mark merged, then fan out to this node's replicators, still subject to the
//! replication policy's `may_send`.

use std::sync::Arc;

use async_trait::async_trait;
use blockstore::Blockstore;
use p2p::sync::SyncCoordinator;
use p2p::transport::P2PTransport;

use crate::merge::governance::{RedrivenMerge, RedrivenMergeSink};

pub struct SyncRedrivenSink<
    B: Blockstore + defra_core::thread_bounds::MaybeSendSync + 'static,
    T: P2PTransport + 'static,
> {
    sync: Arc<SyncCoordinator<B, T>>,
}

impl<
        B: Blockstore + defra_core::thread_bounds::MaybeSendSync + 'static,
        T: P2PTransport + 'static,
    > SyncRedrivenSink<B, T>
{
    pub fn new(sync: Arc<SyncCoordinator<B, T>>) -> Self {
        Self { sync }
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl<B, T> RedrivenMergeSink for SyncRedrivenSink<B, T>
where
    B: Blockstore + defra_core::thread_bounds::MaybeSendSync + 'static,
    T: P2PTransport + 'static,
{
    async fn forward(&self, merged: RedrivenMerge) {
        if let Err(error) = self.sync.mark_as_merged(&merged.cid).await {
            tracing::warn!(cid = %merged.cid, %error, "Re-driven merge not marked merged");
        }
        if merged.collection_id.is_empty() {
            return;
        }
        if let Err(error) = self
            .sync
            .push_to_replicators_with_creator(
                &merged.cid,
                &merged.block_data,
                &merged.doc_id,
                &merged.collection_id,
                (!merged.creator.is_empty()).then_some(merged.creator.as_str()),
            )
            .await
        {
            tracing::error!(
                cid = %merged.cid,
                doc_id = %merged.doc_id,
                collection_id = %merged.collection_id,
                %error,
                "Re-driven merge not forwarded to replicators"
            );
        }
    }
}
