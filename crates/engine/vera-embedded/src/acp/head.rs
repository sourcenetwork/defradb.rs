//! The newest finalized block, as the first header of a subscription.
//!
//! The node streams the headers of blocks that finalize after a light client
//! subscribes, and nothing from before. A fresh deployment finalizes its
//! first block as it starts, before the provider's light client subscribes,
//! and a lone validator proposes the next one only for a request or at its
//! heartbeat: the light client would wait that long for a first verified
//! state. The subscription therefore starts with the node's newest finalized
//! block. The header is built from the node's light block and is as
//! untrusted as any streamed one: the light client takes it only after the
//! block's finalization certificate verifies against its trusted consensus
//! key (`header_sync` in acp-light-client), and refuses it otherwise.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use commonware_codec::Decode as _;
use serde_json::{json, Value};
use vera_domain::{Block, BlockCfg, GossipHeader, LightBlock, TxCfg, MAX_BLOCK_TXS, MAX_TX_BYTES};

use crate::call::call;
use crate::deadline;
use crate::EmbeddedTransport;

const NODE_STATUS: &str = "vera_nodeStatus";
const LIGHT_BLOCK: &str = "vera_getLightBlock";

/// The most bytes of a node status answer read.
const STATUS_BYTES: usize = 64 << 10;

/// The most bytes of a light block answer read: the limit acp-light-client
/// reads one with (`LIGHT_BLOCK_RESPONSE_BYTES`).
const LIGHT_BLOCK_BYTES: usize = 4 << 20;

/// How old a head may be and still be sent: the light client's default
/// freshness limit (`FreshnessPolicy::default().max_age`). An older one would
/// only be refused, and the node's next block is on its way after a restart.
const FRESH_HEAD: Duration = Duration::from_secs(30);

/// The header of the node's newest finalized block, when it has one recent
/// enough for a light client to take. Any failure to read it leaves the
/// subscription as the node streams it.
pub(super) async fn finalized(transport: &EmbeddedTransport) -> Option<GossipHeader> {
    let status = call(
        transport,
        NODE_STATUS,
        json!([]),
        STATUS_BYTES,
        deadline::PROOF_READ,
    )
    .await
    .ok()?;
    let height = status.get("finalizedHeight").and_then(Value::as_u64)?;
    let chain_id = status.get("chainId").and_then(Value::as_u64)?;
    if height == 0 {
        return None;
    }
    let light = call(
        transport,
        LIGHT_BLOCK,
        json!([height]),
        LIGHT_BLOCK_BYTES,
        deadline::PROOF_READ,
    )
    .await
    .ok()?;
    let light: LightBlock = serde_json::from_value(light).ok()?;
    let header = header(&light, chain_id)?;
    recent(header.timestamp, SystemTime::now()).then_some(header)
}

/// The header of the block `light` carries, as the node would stream it.
fn header(light: &LightBlock, chain_id: u64) -> Option<GossipHeader> {
    let encoded = hex::decode(light.block.strip_prefix("0x").unwrap_or(&light.block)).ok()?;
    let block = Block::decode_cfg(
        commonware_codec::Copying(encoded.as_slice()),
        &BlockCfg {
            max_txs: MAX_BLOCK_TXS,
            tx: TxCfg {
                max_tx_bytes: MAX_TX_BYTES,
            },
        },
    )
    .ok()?;
    let hash = light.block_hash.parse().ok()?;
    (block.height == light.height)
        .then(|| GossipHeader::from_block_with_hash(&block, hash, chain_id, 0))
}

/// Whether a block made at `timestamp`, in Unix seconds, is within
/// [`FRESH_HEAD`] of `now`.
fn recent(timestamp: u64, now: SystemTime) -> bool {
    let now = now.duration_since(UNIX_EPOCH).unwrap_or_default();
    now.saturating_sub(Duration::from_secs(timestamp)) <= FRESH_HEAD
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_head_older_than_the_light_clients_freshness_limit_is_not_sent() {
        let now = UNIX_EPOCH + Duration::from_secs(1_000);
        assert!(recent(1_000, now));
        assert!(recent(1_000 - FRESH_HEAD.as_secs(), now));
        assert!(!recent(1_000 - FRESH_HEAD.as_secs() - 1, now));
        // A block stamped ahead of this clock is the light client's to judge.
        assert!(recent(1_010, now));
        assert_eq!(
            FRESH_HEAD,
            acp_light_client::FreshnessPolicy::default().max_age
        );
    }
}
