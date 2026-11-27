//! Blocks as the caller `context` names: their signatures verified, and the
//! signed block material read past document access control.

use defra_core::RequestContext;
use query::TransactionHandle;

use super::NodeCore;
use crate::Result;

impl NodeCore<'_> {
    /// Check that block `cid` was signed by `public_key` (hex, of `key_type`),
    /// read as `context`'s identity, in the transaction it carries if it
    /// carries one (Go: `client.Store::VerifySignature`).
    pub(crate) async fn verify_block_signature(
        &self,
        context: &RequestContext,
        cid: &str,
        public_key: &str,
        key_type: crypto::KeyType,
    ) -> Result<()> {
        self.node
            .ops()
            .verify_block_signature(
                &self.node.admitted_caller(context)?,
                cid,
                public_key,
                key_type,
            )
            .await
    }

    /// Cryptographically verify block `cid` and return its signer DID: the
    /// signature it links is verified over the canonical DAG-CBOR block bytes
    /// and the DID derived from the verified public key. Unsigned, malformed,
    /// missing, or invalid blocks fail closed. In the transaction `context`
    /// carries, if any, it sees the transaction's uncommitted blocks.
    pub(crate) async fn verified_block_signer_did(
        &self,
        context: &RequestContext,
        cid: &str,
    ) -> Result<String> {
        if let Some(id) = context.txn() {
            let transaction = TransactionHandle::new(id.to_string());
            return self
                .node
                .block_ops
                .verified_signer_did_in_txn(&self.node.node_context(), cid, &transaction)
                .await;
        }
        let (ops, cid) = (self.node.block_ops.clone(), cid.to_owned());
        self.node
            .on_pool(move || async move { ops.verified_signer_did(&cid).await })
            .await
    }

    /// Load canonical signed-block material after applying document ACP for
    /// the identity `context` names. The caller must independently verify
    /// both CIDs and the detached signature before trusting the returned
    /// signer.
    pub(crate) async fn authorized_signed_block_bytes(
        &self,
        context: &RequestContext,
        cid: &str,
    ) -> Result<(Vec<u8>, Vec<u8>)> {
        let (ops, cid) = (self.node.block_ops.clone(), cid.to_owned());
        let caller = context.identity().map(str::to_owned);
        self.node
            .on_pool(move || async move { ops.signed_block_bytes(&cid, caller.as_deref()).await })
            .await
    }
}
