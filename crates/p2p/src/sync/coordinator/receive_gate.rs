//! The two receive-path checks on pushed documents: one when a push
//! arrives, before its head is stored or anything fetched, and one on a
//! fetched CAR, before its blocks are imported. `proofs/tla/ReceiveGate.tla`
//! models them.

use async_trait::async_trait;
use blockstore::{verify_block_cid, Blockstore};
use bytes::Bytes;
use cid::Cid;

use super::SyncCoordinator;
use crate::message::PushLogBroadcast;
use crate::sync::receive_gate::{CarBlocks, GateBlocks, SharedReceiveGate};
use crate::transport::P2PTransport;

/// Blocks this node already stores.
struct StoredBlocks<'a, B>(&'a B);

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl<B> GateBlocks for StoredBlocks<'_, B>
where
    B: Blockstore,
{
    async fn block(&self, cid: &Cid) -> Option<Bytes> {
        self.0.get(cid).await.ok().flatten()
    }
}

impl<B, T> SyncCoordinator<B, T>
where
    B: Blockstore + 'static,
    T: P2PTransport,
{
    /// Check every pushed document with `gate` before its blocks are fetched
    /// or stored. The merge keeps its own check as the backstop.
    pub fn set_receive_gate(&self, gate: SharedReceiveGate) {
        let _ = self.manager.receive_gate.set(gate);
    }

    /// The check on a push as it arrives, on its head and this node's
    /// store. `false` when the gate refused it: nothing of it is stored and
    /// nothing fetched. A head whose bytes do not match its CID goes on, so
    /// the ingest refuses it as before.
    pub(super) async fn gate_arrival(&self, message: &PushLogBroadcast) -> bool {
        let Some(gate) = self.manager.receive_gate.get() else {
            return true;
        };
        let Ok(head) = Cid::try_from(message.cid.as_ref()) else {
            return true;
        };
        if verify_block_cid(&head, &message.block).is_err() {
            return true;
        }
        let stored = StoredBlocks(self.manager.blockstore().as_ref());
        if gate
            .allows(
                &message.collection_id,
                &message.doc_id,
                &message.block,
                &stored,
            )
            .await
        {
            return true;
        }
        tracing::debug!(
            head = %head,
            doc_id = %message.doc_id,
            collection_id = %message.collection_id,
            "Pushed document refused before anything was fetched"
        );
        false
    }

    /// The check on a CAR fetched for `root`, before its blocks are
    /// imported. `false` when the gate refused the document: the root is
    /// discharged as a terminal skip, as the merge discharges a refusal, so
    /// it is not fetched again.
    pub(super) async fn gate_fetched(&self, root: &Cid, blocks: &[(Cid, Vec<u8>)]) -> bool {
        let Some(gate) = self.manager.receive_gate.get() else {
            return true;
        };
        let Some(pending) = self.manager.pending_dag_snapshot(root) else {
            return true;
        };
        let stored = StoredBlocks(self.manager.blockstore().as_ref());
        let car = CarBlocks::new(
            blocks
                .iter()
                .map(|(cid, data)| (*cid, Bytes::copy_from_slice(data))),
            Some(&stored),
        );
        let Some(head) = car.block(root).await else {
            return true;
        };
        if gate
            .allows(&pending.collection_id, &pending.doc_id, &head, &car)
            .await
        {
            return true;
        }
        tracing::debug!(
            root = %root,
            doc_id = %pending.doc_id,
            collection_id = %pending.collection_id,
            "Fetched document refused before its CAR was imported"
        );
        if let Err(error) = self.manager.mark_as_merged(root).await {
            tracing::warn!(root = %root, %error, "Could not discharge a refused root");
        }
        false
    }
}
