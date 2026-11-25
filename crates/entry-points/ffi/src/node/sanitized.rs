//! The node's options as Go's `SanitizedMap` reports them: every value that
//! could locate or unlock the node (a path, an address, a key) redacted.

use serde_json::{json, Value};

use super::options::NodeSetup;
use crate::types::NodeInitOptions;

/// What a redacted value reads as.
const REDACTED: &str = "<redacted>";

fn seconds_as_nanos(seconds: u64) -> u64 {
    seconds.saturating_mul(1_000_000_000)
}

/// `report` with what only the built `node` knows: the transaction retries
/// it resolved, from the caller's value or its default.
pub(crate) fn with_node(mut report: Value, node: &defra_node::Node) -> Value {
    report["DB"]["MaxTxnRetries"] = json!(node.database().options().max_txn_retries());
    report
}

/// The sanitized options of a node built from `options` and `setup`, each
/// value as the node runs with it: the caller's, or the default it resolves
/// to. [`with_node`] fills in the values only the built node knows.
///
/// # Safety
///
/// Every non-null string in `options` must be a valid null-terminated string.
pub(crate) unsafe fn options_json(
    options: &NodeInitOptions,
    setup: &NodeSetup,
    transport: &defra_node::TransportConfig,
) -> Value {
    let redacted = |present: bool| present.then_some(REDACTED);
    let redacted_text = |present: bool| if present { REDACTED } else { "" };
    let redacted_list = |values: &[String]| vec![REDACTED; values.len()];
    let config = &setup.config;
    let persistent = matches!(setup.store, super::options::StoreLocation::Path(_));

    let (listen_addrs, peers, topics) = match transport {
        #[cfg(feature = "p2p")]
        defra_node::TransportConfig::Pinc(pinc) => (
            vec![pinc.bind.to_string()],
            pinc.bootstrap_peers.clone(),
            true,
        ),
        _ => (Vec::new(), Vec::new(), false),
    };
    let document_acp_type = match config.document_acp {
        defra_node::DocumentAcpConfig::Local => "local",
        #[allow(unreachable_patterns)]
        _ => "remote",
    };

    json!({
        "DisableP2P": matches!(transport, defra_node::TransportConfig::None),
        "DisableAPI": true,
        "Store": {
            "Store": if persistent { "regolith" } else { "memory" },
            "Path": redacted_text(persistent),
            "BadgerFileSize": setup.store_file_size.unwrap_or_else(|| {
                storage::RegolithStoreOptions::default()
                    .engine
                    .get_target_file_size()
            }),
            "BadgerEncryptionKey": redacted(setup.store_encryption_key.is_some()),
            "BadgerInMemory": !persistent,
        },
        "DocumentACP": {
            "DocumentACPType": document_acp_type,
            "Path": redacted_text(persistent),
        },
        "NodeACP": {
            "IsEnabled": config.node_acp_enabled,
            "Path": redacted_text(config.node_acp_enabled && persistent),
        },
        "DB": {
            "MaxTxnRetries": config.max_txn_retries,
            "Identity": redacted(options.identity_ptr != 0),
            "EnableSigning": setup.signing_enabled,
            "SearchableEncryptionKey": redacted(setup.searchable_encryption_key.is_some()),
            "RetryIntervals": super::transport::retry_intervals_of(transport)
                .unwrap_or(storage::stores::RETRY_INTERVALS_SECS)
                .iter()
                .copied()
                .map(seconds_as_nanos)
                .collect::<Vec<_>>(),
            "P2PBlockSyncTimeout": Value::Null,
            "LensRuntime": "wasmtime",
            "LensPoolSize": 0,
            "ChunkSize": config.chunk_size,
        },
        "P2P": {
            "ListenAddresses": redacted_list(&listen_addrs),
            "BootstrapPeers": redacted_list(&peers),
            "EnablePubSub": topics,
            "EnableRelay": topics,
            "EnableClearBackoffOnRetry": false,
            "PrivateKey": redacted(setup.peer_key.is_some()),
        },
        "HTTP": {
            "Address": "",
            "AllowedOrigins": Vec::<String>::new(),
            "TLSCertPath": "",
            "TLSKeyPath": "",
        },
    })
}
