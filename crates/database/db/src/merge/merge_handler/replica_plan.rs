//! What the merge does about a frame of a collection whose replicated
//! documents this node registers with a remote provider
//! ([`crate::merge::replica`]).

use super::*;
use crate::merge::replica::ReplicaStep;

impl<S: Store, B: blockstore::Blockstore> DbMergeHandler<S, B> {
    /// Plan one frame before its transaction opens. A genesis is judged by
    /// the DID its own signature verifies to; a signature that does not
    /// verify names nobody, and the usual gates decide.
    pub(crate) async fn plan_replica_frame(
        &self,
        cid: &Cid,
        block: &Block,
        doc_id: &str,
        collection: &CollectionVersion,
    ) -> std::result::Result<ReplicaStep, MergeError> {
        let Some(hook) = self.composite_merge_hook() else {
            return Ok(ReplicaStep::Gate);
        };
        if !hook.plans_replicas(collection) {
            return Ok(ReplicaStep::Gate);
        }
        let is_genesis = block.heads.as_deref().is_none_or(<[Cid]>::is_empty);
        let signer = if is_genesis {
            match self.verify_block_signature(cid, block, &[]).await {
                Ok(signer) => signer,
                Err(error) => {
                    tracing::debug!(%cid, %error, "Genesis signature did not verify");
                    None
                }
            }
        } else {
            None
        };
        hook.plan_replica(doc_id, collection, is_genesis, signer.as_deref())
            .await
    }
}
