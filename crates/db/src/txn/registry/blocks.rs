//! Block signature verification inside a transaction.

use super::*;

impl<S: Store + 'static> DbTransactionRegistry<S> {
    /// [`Self::verify_block_signature_in_txn_as`] in [`RequestContext::ambient`].
    pub async fn verify_block_signature_in_txn(
        &self,
        txn_id: &str,
        document_acp: Arc<dyn acp::DocumentACP>,
        cid_str: &str,
        public_key_hex: &str,
        key_type: crypto::KeyType,
        caller_identity: &acp::Identity,
    ) -> Result<()> {
        self.verify_block_signature_in_txn_as(
            &RequestContext::ambient(),
            txn_id,
            document_acp,
            cid_str,
            public_key_hex,
            key_type,
            caller_identity,
        )
        .await
    }

    /// Verify a block signature using the existing transaction's blockstore view.
    #[allow(clippy::too_many_arguments)]
    pub async fn verify_block_signature_in_txn_as(
        &self,
        context: &RequestContext,
        txn_id: &str,
        document_acp: Arc<dyn acp::DocumentACP>,
        cid_str: &str,
        public_key_hex: &str,
        key_type: crypto::KeyType,
        caller_identity: &acp::Identity,
    ) -> Result<()> {
        let db = self.db.clone();
        let request = context.clone();
        let (cid_str, public_key_hex, caller_identity) = (
            cid_str.to_owned(),
            public_key_hex.to_owned(),
            caller_identity.clone(),
        );
        self.in_txn(context, txn_id, move |owned| {
            Box::pin(async move {
                let txn = owned.alone()?;
                crate::block::verify::verify_block_signature_in_txn_as(
                    &request,
                    &db,
                    document_acp.as_ref(),
                    txn,
                    &cid_str,
                    &public_key_hex,
                    key_type,
                    &caller_identity,
                )
                .await
                .map_err(Error::Other)
            })
        })
        .await
    }

    /// [`Self::verified_block_signer_did_in_txn_as`] in [`RequestContext::ambient`].
    pub async fn verified_block_signer_did_in_txn(
        &self,
        txn_id: &str,
        document_acp: Arc<dyn acp::DocumentACP>,
        cid_str: &str,
        caller_identity: &acp::Identity,
    ) -> Result<String> {
        self.verified_block_signer_did_in_txn_as(
            &RequestContext::ambient(),
            txn_id,
            document_acp,
            cid_str,
            caller_identity,
        )
        .await
    }

    /// Verify a block's embedded signature inside an active transaction and
    /// return the signer DID.
    pub async fn verified_block_signer_did_in_txn_as(
        &self,
        context: &RequestContext,
        txn_id: &str,
        document_acp: Arc<dyn acp::DocumentACP>,
        cid_str: &str,
        caller_identity: &acp::Identity,
    ) -> Result<String> {
        let db = self.db.clone();
        let dac_bypass = context.dac_bypass();
        let (cid_str, caller_identity) = (cid_str.to_owned(), caller_identity.clone());
        self.in_txn(context, txn_id, move |owned| {
            Box::pin(async move {
                let txn = owned.alone()?;
                crate::block::verify::verified_block_signer_did_with_txn(
                    &db,
                    document_acp.as_ref(),
                    txn,
                    &cid_str,
                    &caller_identity,
                    dac_bypass,
                )
                .await
                .map_err(Error::Other)
            })
        })
        .await
    }
}
