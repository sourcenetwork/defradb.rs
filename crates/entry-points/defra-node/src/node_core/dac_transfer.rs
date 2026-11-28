//! Giving a document to another owner as an embedder-authenticated actor in
//! a node-owned transaction (`proofs/tla/OwnerTransfer.tla`).

use std::sync::Arc;

use defra_core::{RequestContext, TxnId};

use super::NodeCore;
use crate::acp_ops::transfer::Transfer;
use crate::Result;

impl NodeCore<'_> {
    /// Give document `doc_id` to `new_owner` in the node-owned transaction
    /// `id`, as `actor`, which must own it. Transaction ownership is checked
    /// as the node; native NAC and DAC check `actor`. On local document ACP
    /// the owner tuple is replaced in the transaction; on a remote provider
    /// the transfer is owed and sent after the commit.
    pub(crate) async fn transfer_dac_object_owner_for(
        &self,
        id: TxnId,
        actor: &RequestContext,
        collection: &str,
        doc_id: &str,
        new_owner: &str,
    ) -> Result<()> {
        let transaction_context = self.node.node_context().with_txn(Some(id));
        let actor = actor.clone();
        let ops = Arc::clone(&self.node.acp_ops);
        let (collection, doc_id, new_owner) = (
            collection.to_owned(),
            doc_id.to_owned(),
            new_owner.to_owned(),
        );
        self.node
            .on_acp(move || async move {
                ops.transfer_dac_owner_for_in_txn(
                    &transaction_context,
                    &actor,
                    Transfer {
                        collection: &collection,
                        doc_id: &doc_id,
                        new_owner: &new_owner,
                    },
                )
                .await
            })
            .await
    }
}
