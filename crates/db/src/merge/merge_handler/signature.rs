//! Block signature verification for merge operations.

use cid::Cid;
use defra_core::block::Block;
use storage::corekv::Store;

use super::error::MergeError;
use super::DbMergeHandler;

impl<S: Store, B: blockstore::Blockstore> DbMergeHandler<S, B> {
    /// Verify block signature and return the verified creator identity.
    ///
    /// Returns:
    /// - `Ok(Some(identity))` -- signature valid, identity is the verified signer
    /// - `Ok(None)` -- unsigned block
    /// - `Err(SignatureVerificationFailed)` -- invalid signature, block MUST be rejected
    pub async fn verify_block_signature(
        &self,
        cid: &Cid,
        block: &Block,
        _block_data: &[u8],
    ) -> Result<Option<String>, MergeError> {
        let sig_cid = match &block.signature {
            Some(sig_cid) => sig_cid,
            None => {
                // Unsigned blocks are the default in Go-compatible deployments;
                // warning per block produced hundreds of noisy lines per
                // replicated document (issue #858). Keep at debug.
                tracing::debug!(
                    cid = %cid,
                    "P2P block has no signature — cannot verify authenticity"
                );
                return Ok(None);
            }
        };

        let sig_data = match self.blockstore.get(sig_cid).await {
            Ok(Some(data)) => data,
            Ok(None) => {
                return Err(MergeError::SignatureVerificationFailed {
                    cid: *cid,
                    reason: format!("signature block {} not found in blockstore", sig_cid),
                });
            }
            Err(e) => {
                return Err(MergeError::SignatureVerificationFailed {
                    cid: *cid,
                    reason: format!("failed to load signature block {}: {}", sig_cid, e),
                });
            }
        };

        verify_signature_data(cid, block, &sig_data).map(Some)
    }
}

/// Verify a signature block's raw bytes against a block and return the
/// verified signer DID.
///
/// Shared by the merge-time verification above (signature loaded from the
/// blockstore) and browser-sync validation (signature carried in the push
/// payload).
pub(crate) fn verify_signature_data(
    cid: &Cid,
    block: &Block,
    sig_data: &[u8],
) -> Result<String, MergeError> {
    let signature = match defra_core::block::Signature::from_dag_cbor(sig_data) {
        Ok(sig) => sig,
        Err(e) => {
            return Err(MergeError::SignatureVerificationFailed {
                cid: *cid,
                reason: format!("failed to decode signature block: {}", e),
            });
        }
    };

    crate::block::verify::verified_signature_signer_did(block, &signature)
        .map_err(|reason| MergeError::SignatureVerificationFailed { cid: *cid, reason })
}
