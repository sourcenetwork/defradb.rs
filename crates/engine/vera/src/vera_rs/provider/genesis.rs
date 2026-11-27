//! The deployment's genesis identity, as the light client proves it.
//!
//! A delegation bound to one request names the deployment's genesis
//! identity. vera.rs's node takes the ID of its genesis block as that
//! identity, and its first block names the genesis block as its parent. The
//! provider reads the first block and checks its finality against the
//! trusted consensus key, the trust every other read has: the node's answer
//! alone is never taken.

use std::time::{Duration, Instant};

use acp_light_client::{AcpTransport, ProofClient, SyncState};
use alloy_primitives::B256;
use vera_domain::{AncestryProof, BlockId, ConsensusPublicKey, LightBlock};

use crate::provider::ProviderError;

/// Where a provider takes its deployment's genesis identifier from. The
/// light client follows the deployment across a switch of its active device
/// only when it has one: a later term is taken under its certificate, which
/// names that identifier.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GenesisSource {
    /// Given by the deployment's operators, as its consensus key is: the
    /// configuration of a node that reaches a Vera node over the network.
    Configured([u8; 32]),
    /// Read from the first block of the node this process hosts, checked
    /// against the consensus key: the node is the host's own, which created
    /// the deployment.
    Hosted,
    /// None. The light client refuses every term above 0, so after any
    /// switch of the deployment's active device every verified read fails
    /// until a genesis identifier is configured.
    Unnamed,
}

/// How long a hosted node's first block is asked for between attempts.
const FIRST_BLOCK_RETRY: Duration = Duration::from_millis(200);
const ANCESTRY_RESPONSE_BYTES: usize = 16 << 10;

/// The genesis identity of the deployment `trusted` is the consensus key of,
/// read through `proofs`. Runs inside `edge::compat`.
pub(super) async fn read(
    proofs: &ProofClient,
    trusted: &ConsensusPublicKey,
) -> Result<[u8; 32], ProviderError> {
    let first = proofs
        .get_light_block(1)
        .await
        .map_err(|e| ProviderError::Query(format!("first block: {e}")))?;
    genesis_of(&first, trusted)
}

/// [`read`], asked again until `patience` has passed: a hosted node that
/// has just started finalizes its first block within its heartbeat.
pub(super) async fn read_within(
    proofs: &ProofClient,
    trusted: &ConsensusPublicKey,
    patience: Duration,
) -> Result<[u8; 32], ProviderError> {
    let started = Instant::now();
    loop {
        match read(proofs, trusted).await {
            Ok(genesis) => return Ok(genesis),
            Err(error) if started.elapsed() >= patience => return Err(error),
            Err(_) => nuclei::time::sleep(FIRST_BLOCK_RETRY).await,
        }
    }
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

/// Whether the revision `state` names is proven to descend from `genesis`.
/// The light block is checked under `trusted` and the proof against the
/// ancestry that block commits to, so neither answer is taken from the node.
pub(super) async fn descends_from(
    proofs: &ProofClient,
    transport: &dyn AcpTransport,
    trusted: &ConsensusPublicKey,
    genesis: [u8; 32],
    state: &SyncState,
) -> Result<bool, ProviderError> {
    let value = transport
        .call(
            "vera_getAncestryProof",
            serde_json::json!([0, state.height]),
            ANCESTRY_RESPONSE_BYTES,
        )
        .await
        .map_err(|error| ProviderError::Query(format!("ancestry proof: {error}")))?;
    let proof: AncestryProof = serde_json::from_value(value)
        .map_err(|error| ProviderError::Query(format!("ancestry proof: {error}")))?;
    let light = proofs
        .get_light_block(state.height)
        .await
        .map_err(|error| ProviderError::Query(format!("ancestry block: {error}")))?;
    let block = vera_domain::verify_finalized_block(&light, trusted)
        .map_err(|error| ProviderError::Query(format!("ancestry block: {error}")))?;
    if block.height != state.height || block.id().0 != state.block_hash {
        return Err(ProviderError::Query(
            "ancestry block differs from the verified revision".into(),
        ));
    }
    Ok(block.has_ancestor(0, &BlockId(B256::from(genesis)), &proof))
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
