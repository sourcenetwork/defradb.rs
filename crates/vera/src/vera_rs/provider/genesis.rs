//! The deployment's genesis identity, as the light client proves it.
//!
//! A delegation bound to one request names the deployment's genesis
//! identity. vera.rs's node takes the ID of its genesis block as that
//! identity, and its first block names the genesis block as its parent. The
//! provider reads the first block and checks its finality against the
//! trusted consensus key, the trust every other read has: the node's answer
//! alone is never taken.

use acp_light_client::AcpLightClient;
use vera_domain::{ConsensusPublicKey, LightBlock};

use crate::provider::ProviderError;

/// The genesis identity of the deployment `trusted` is the consensus key of,
/// read through `light_client`. Runs inside `edge::compat`.
pub(super) async fn read(
    light_client: &AcpLightClient,
    trusted: &ConsensusPublicKey,
) -> Result<[u8; 32], ProviderError> {
    let first = light_client
        .proof_client()
        .get_light_block(1)
        .await
        .map_err(|e| ProviderError::Query(format!("first block: {e}")))?;
    genesis_of(&first, trusted)
}

/// The parent of `first`, once `first` is shown to be the deployment's
/// finalized first block under `trusted`.
pub(super) fn genesis_of(
    first: &LightBlock,
    trusted: &ConsensusPublicKey,
) -> Result<[u8; 32], ProviderError> {
    if first.height != 1 {
        return Err(ProviderError::Query(format!(
            "the node answered for block {} when asked for the first",
            first.height
        )));
    }
    let block = vera_domain::verify_finalized_block(first, trusted)
        .map_err(|e| ProviderError::Query(format!("first block: {e}")))?;
    let genesis = block.parent.0 .0;
    if genesis == [0; 32] {
        return Err(ProviderError::Query(
            "the first block names no genesis block".into(),
        ));
    }
    Ok(genesis)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vera_rs::client::tests::TRUSTED_KEY;

    fn unproven(height: u64) -> LightBlock {
        LightBlock {
            block_hash: format!("0x{}", "11".repeat(32)),
            parent_hash: format!("0x{}", "22".repeat(32)),
            height,
            timestamp: 0,
            state_root: format!("0x{}", "00".repeat(32)),
            module_state_root: format!("0x{}", "00".repeat(32)),
            epoch: 0,
            view: 1,
            parent_view: 0,
            block: "0x00".into(),
            descendants: Vec::new(),
            finalization: "0x00".into(),
            epoch_material: "0x00".into(),
        }
    }

    #[test]
    fn a_genesis_identity_is_taken_only_from_a_proven_first_block() {
        let trusted = super::super::trusted_key(TRUSTED_KEY).expect("key");
        // The parent hash the node displays is not taken on its word.
        assert!(matches!(
            genesis_of(&unproven(1), &trusted),
            Err(ProviderError::Query(_))
        ));
        assert!(matches!(
            genesis_of(&unproven(2), &trusted),
            Err(ProviderError::Query(message)) if message.contains("block 2")
        ));
    }
}
