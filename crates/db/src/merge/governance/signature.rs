use cid::Cid;
use defra_core::block::Block;
use storage::corekv::Store;

use crate::merge::merge_handler::{verify_signature_data, DbMergeHandler, MergeError};

/// What a composite's own signature proves, judged from the block and its
/// signature block alone, never from the metadata that carried it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SignatureStatus {
    Unsigned,
    /// The signature verified; the value is the signer's DID.
    Verified(String),
    /// The signature block is held and does not verify this block.
    Invalid(String),
    /// The block names a signature block this node does not hold.
    NotHeld(Cid),
}

impl<S: Store, B: blockstore::Blockstore> DbMergeHandler<S, B> {
    /// A storage read failure is an error, not an invalid signature, so it
    /// cannot turn into a permanent reject.
    pub(crate) async fn frame_signature(
        &self,
        cid: &Cid,
        block: &Block,
    ) -> Result<SignatureStatus, MergeError> {
        let Some(signature_cid) = block.signature else {
            return Ok(SignatureStatus::Unsigned);
        };
        let signature = match self.blockstore.get(&signature_cid).await {
            Ok(Some(data)) => data,
            Ok(None) => return Ok(SignatureStatus::NotHeld(signature_cid)),
            Err(error) => return Err(MergeError::Storage(error.to_string())),
        };
        Ok(match verify_signature_data(cid, block, &signature) {
            Ok(did) => SignatureStatus::Verified(did),
            Err(error) => SignatureStatus::Invalid(error.to_string()),
        })
    }
}
