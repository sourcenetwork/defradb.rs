//! Per-frame write authorization for updates to protected documents.

use super::hook::CompositeFrame;
use super::*;

impl<S: Store, B: blockstore::Blockstore> DbMergeHandler<S, B> {
    /// Judge one composite by its own signature. Ancestors loaded from the
    /// blockstore and composites linked from a collection block arrive with
    /// the carrier's metadata, whose signer says nothing about this block.
    pub(crate) async fn check_protected_update(
        &self,
        cid: &Cid,
        block: &Block,
        payload: &defra_core::block::CompositeDeltaPayload,
        doc_id: &str,
        collection: &CollectionVersion,
    ) -> std::result::Result<Option<MergeOutcome>, MergeError> {
        let Some(hook) = self.composite_merge_hook() else {
            return Ok(None);
        };
        let is_genesis = block.heads.as_deref().is_none_or(<[Cid]>::is_empty);
        if collection.policy.is_none() || is_genesis || !hook.guards_protected_updates() {
            return Ok(None);
        }

        // A signature that fails to verify names no one, so it is judged as
        // unsigned; the root's own failure is already refused at dispatch.
        let signer = match self.verify_block_signature(cid, block, &[]).await {
            Ok(signer) => signer,
            Err(error) => {
                tracing::debug!(%cid, %error, "Composite signature did not verify");
                None
            }
        };

        hook.on_protected_update(
            doc_id,
            collection,
            CompositeFrame {
                is_genesis,
                status: payload.status,
                signer: signer.as_deref(),
            },
        )
        .await
    }
}
