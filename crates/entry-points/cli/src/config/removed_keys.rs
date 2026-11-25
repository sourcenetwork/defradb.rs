//! `acp` keys of the removed Go-chain ACP provider and of vera.rs's old names.
//!
//! serde ignores a key no field reads, so an old `config.yaml` would load
//! without a word and the operator would meet a later "vera_address
//! required" instead of the reason.

use serde_yaml::Value;

/// A removed `acp` key and what the operator does instead.
const REMOVED_ACP_KEYS: &[(&str, &str)] = &[
    ("vera_rs_address", "set acp.vera_address instead"),
    ("hub_rs_address", "set acp.vera_address instead"),
    ("sourcehub_address", GO_CHAIN_REMOVED),
    ("sourcehub_grpc_address", GO_CHAIN_REMOVED),
    ("sourcehub_comet_address", GO_CHAIN_REMOVED),
    ("sourcehub_events_ws", GO_CHAIN_REMOVED),
    ("sourcehub_chain_id", GO_CHAIN_REMOVED),
    ("vera_grpc_address", GO_CHAIN_REMOVED),
    ("vera_comet_address", GO_CHAIN_REMOVED),
    ("vera_events_ws", GO_CHAIN_REMOVED),
    ("vera_chain_id", GO_CHAIN_REMOVED),
    ("circuit_breaker_threshold", GO_CHAIN_REMOVED),
    ("circuit_breaker_reset_timeout", GO_CHAIN_REMOVED),
    ("cache_ttl", GO_CHAIN_REMOVED),
];

const GO_CHAIN_REMOVED: &str =
    "the Go-chain ACP provider was removed; acp.vera_address names a vera.rs node";

/// Drops each of [`REMOVED_ACP_KEYS`] that `root`'s `acp` section sets, its
/// keys already lowercased, with a warning pushed onto `warnings`.
pub(super) fn apply(root: &mut Value, warnings: &mut Vec<String>) {
    let Some(Value::Mapping(acp)) = root.get_mut("acp") else {
        return;
    };
    for (key, instead) in REMOVED_ACP_KEYS {
        if acp.remove(*key).is_some() {
            warnings.push(format!("acp.{key} is ignored: {instead}"));
        }
    }
}
