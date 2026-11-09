//! Block signatures as the caller `context` names.

use defra_core::RequestContext;

use super::handle::NodeOps;
use super::request::acting_identity;
use crate::{Error, Node, Result};

impl Node {
    /// Check that block `cid` was signed by `public_key` (hex, of `key_type`),
    /// read as `context`'s identity, in the transaction it carries if it
    /// carries one (Go: `client.Store::VerifySignature`).
    pub async fn verify_block_signature_as(
        &self,
        context: &RequestContext,
        cid: &str,
        public_key: &str,
        key_type: crypto::KeyType,
    ) -> Result<()> {
        self.ops()
            .verify_block_signature(&self.caller(context), cid, public_key, key_type)
            .await
    }
}

impl NodeOps {
    /// [`Node::verify_block_signature_as`] as `context` names, with no
    /// default caller.
    pub(crate) async fn verify_block_signature(
        &self,
        context: &RequestContext,
        cid: &str,
        public_key: &str,
        key_type: crypto::KeyType,
    ) -> Result<()> {
        // NAC, the block read and document ACP decide at one view: the
        // caller's transaction, or the read-only one the pool path opens.
        let caller: acp::Identity = acting_identity(context)?.into();
        if let Some(txn) = context.txn() {
            return self
                .txn_registry
                .verify_block_signature_in_txn_as(
                    context,
                    &txn.to_string(),
                    self.document_acp.clone(),
                    cid,
                    public_key,
                    key_type,
                    &caller,
                )
                .await
                .map_err(|error| Error::BlockAccess(error.to_string()));
        }
        let (database, acp, context) = (
            self.database.clone(),
            self.document_acp.clone(),
            context.clone(),
        );
        let (cid, public_key) = (cid.to_owned(), public_key.to_owned());
        self.on_pool(move || async move {
            db::block::verify::verify_block_signature_as(
                &context,
                &database,
                acp.as_ref(),
                &cid,
                &public_key,
                key_type,
                &caller,
            )
            .await
            .map_err(Error::BlockAccess)
        })
        .await
    }
}
