//! Node lifecycle: create, close, and report options.

pub(crate) mod options;
mod peer_key;
mod sanitized;
mod transport;

#[cfg(test)]
mod tests;

use std::sync::Arc;

use kovan::AtomOption;

use crate::ffi_entry;
use crate::state::{NodeState, P2PState, NODES, TXNS};
use crate::target::{resolve_node, ERR_INVALID_HANDLE};
use crate::try_ffi;
use crate::types::{DefraNewNodeResult, DefraResult, NodeInitOptions};

use options::{NodeSetup, StoreLocation};

/// Creates and starts an embedded DefraDB node.
///
/// # Safety
///
/// Every non-null pointer in `options` must be valid for the call: strings
/// null-terminated, byte buffers at least their stated length.
#[no_mangle]
pub unsafe extern "C" fn defra_node_new(options: NodeInitOptions) -> DefraNewNodeResult {
    ffi_entry! {
        let Some(rt) = crate::runtime::edge() else {
            return DefraNewNodeResult::error(crate::helpers::ERR_RUNTIME_NOT_INITIALIZED);
        };
        let prepared = options::read(&options).and_then(|setup| {
            let transport = transport::read(&options)?;
            let options_json = sanitized::options_json(&options, &setup, &transport);
            Ok((setup, transport, options_json))
        });
        let (setup, transport, options_json) = match prepared {
            Ok(prepared) => prepared,
            Err(error) => return DefraNewNodeResult::error(error),
        };

        let result = crate::helpers::block_on(
            rt,
            build_node_state(setup, transport, options_json, rt.clone()),
        )
        .map(|state| NODES.insert(state));

        match result {
            Ok(handle) => DefraNewNodeResult::success(handle),
            Err(error) => DefraNewNodeResult::error(error),
        }
    }
}

/// Builds a node whose network work, P2P included, runs on `edge`.
pub(crate) async fn build_node_state(
    setup: NodeSetup,
    transport: defra_node::TransportConfig,
    options_json: serde_json::Value,
    edge: edge::Edge,
) -> Result<NodeState, String> {
    let NodeSetup {
        store,
        store_file_size,
        store_encryption_key,
        mut config,
        signing_enabled,
        searchable_encryption_key,
        peer_key,
    } = setup;

    let db_path = match store {
        StoreLocation::Memory => None,
        StoreLocation::Path(path) => Some(path),
    };
    config.transport = with_derived_iroh_key_path(transport, &db_path);
    config.edge = Some(edge);
    if let Some(key) = peer_key {
        config.peer_keyring = Some(defra_node::PeerKeyring::new(Arc::new(
            peer_key::CallerPeerKey::new(key),
        )));
    }

    // A persistent store opens with the server profile. The embedding process
    // is this node's only caller, so its transactions serve whichever identity
    // a call acts as, as in Go.
    let mut builder = defra_node::NodeBuilder::default()
        .with_config(config)
        .with_store_profile(defra_node::StoreProfile::Server)
        .with_txn_ownership(db::TxnOwnership::Embedder);
    if let Some(path) = &db_path {
        builder = builder.data_path(path);
    }
    if let Some(bytes) = store_file_size {
        let mut options = storage::RegolithStoreOptions::default();
        options.engine = options.engine.target_file_size(bytes);
        builder = builder.with_regolith_options(options);
    }
    if let Some(key) = store_encryption_key {
        builder = builder.with_at_rest_encryption_key(*key);
    }
    let node = builder.build().await.map_err(|error| error.to_string())?;

    let state = NodeState {
        database: node.database().clone(),
        background_tasks: node.background_tasks(),
        query_runner: node.runner().clone(),
        nac_manager: node.nac_manager().clone(),
        document_acp: node.document_acp(),
        event_bus: node.event_bus().clone(),
        local_zanzibar_store: node.local_zanzibar_store().cloned(),
        p2p: node
            .p2p_system()
            .cloned()
            .map(|system| Arc::new(P2PState::new(system))),
        node_identity_did: node
            .node_identity_did()
            .map(str::to_owned)
            .map_or_else(AtomOption::none, AtomOption::some),
        signing_enabled,
        query_limits: node.query_limits(),
        se_encryption_key: searchable_encryption_key
            .map_or_else(AtomOption::none, AtomOption::some),
        options_json,
        _node: node,
    };
    state.sync_replicator_push_options()?;
    Ok(state)
}

fn with_derived_iroh_key_path(
    transport: defra_node::TransportConfig,
    _db_path: &Option<String>,
) -> defra_node::TransportConfig {
    #[cfg(feature = "iroh")]
    if let defra_node::TransportConfig::Iroh(mut config) = transport {
        if config.secret_key_path.is_none() {
            if let Some(path) = _db_path {
                config.secret_key_path = Some(std::path::PathBuf::from(format!("{path}.iroh.key")));
            }
        }
        return defra_node::TransportConfig::Iroh(config);
    }

    transport
}

/// Closes the node and releases its handle, with every transaction and
/// subscription still open on it.
#[no_mangle]
pub extern "C" fn defra_node_close(node_ptr: usize) -> DefraResult {
    ffi_entry! {
        use crate::state::GRAPHQL_SUBSCRIPTIONS;

        let rt = try_ffi!(crate::helpers::get_rt());
        try_ffi!(resolve_node(node_ptr));

        for sub_state in GRAPHQL_SUBSCRIPTIONS.remove_for_node(node_ptr) {
            sub_state.service.stop();
            NODES.get(node_ptr, |state| {
                state.event_bus.unsubscribe(sub_state.event_sub_id);
            });
        }
        TXNS.remove_for_node(node_ptr);

        let state = match NODES.remove(node_ptr) {
            Some(state) => state,
            None => return DefraResult::error(ERR_INVALID_HANDLE),
        };

        // The node's own shutdown stops its tasks and transport and closes the
        // database; the P2P system's also clears the signing identities FFI
        // registered, and the event bus is FFI's to close.
        let result = crate::helpers::block_on(rt, async {
            if let Some(p2p) = state.p2p.as_ref() {
                p2p.system.shutdown().await;
            }
            let closed = state._node.shutdown().await;
            state.event_bus.close();
            closed.map_err(|error| format!("failed to close database: {}", error))
        });
        drop(state);

        match result {
            Ok(()) => DefraResult::ok(),
            Err(error) => DefraResult::error(error),
        }
    }
}

/// Returns the node's configuration options as JSON, with every value that
/// could locate or unlock the node redacted.
#[no_mangle]
pub extern "C" fn defra_node_options(node_ptr: usize) -> DefraResult {
    ffi_entry! {
        try_ffi!(resolve_node(node_ptr));
        match NODES.get(node_ptr, |state| state.options_json.to_string()) {
            Some(json) => DefraResult::success(json),
            None => DefraResult::error(ERR_INVALID_HANDLE),
        }
    }
}
