//! Node lifecycle management for FFI.

use std::sync::Arc;

use kovan::AtomOption;

use crate::ffi_entry;
use crate::state::{NodeState, P2PState, PolicyStore, NODES};
use crate::try_ffi;
use crate::types::{c_str_to_string, FfiResult, NewNodeResult, NodeInitOptions};
use crate::ERR_INVALID_NODE_HANDLE;

/// Maximum length for private key byte slices passed via FFI.
pub(crate) const MAX_PRIVATE_KEY_LEN: usize = 128;

/// Create a new DefraDB node without P2P.
#[no_mangle]
pub extern "C" fn new_node(options: NodeInitOptions) -> NewNodeResult {
    ffi_entry! {
        let rt = match crate::runtime::edge() {
            Some(rt) => rt,
            None => return NewNodeResult::error(crate::helpers::ERR_RUNTIME_NOT_INITIALIZED),
        };

        let result = crate::helpers::block_on(
            rt,
            build_node_state(options, defra_node::TransportConfig::None, rt.clone()),
        )
        .map(|state| NODES.insert(state));

        match result {
            Ok(handle) => NewNodeResult::success(handle),
            Err(error) => NewNodeResult::error(error),
        }
    }
}

/// Builds a node whose network work, P2P included, runs on `edge`.
pub(crate) async fn build_node_state(
    options: NodeInitOptions,
    transport: defra_node::TransportConfig,
    edge: edge::Edge,
) -> Result<NodeState, String> {
    let db_path_opt = resolve_store(&options)?;
    let mut config = resolve_embedded_config(&options)?;
    config.transport = with_derived_iroh_key_path(transport, &db_path_opt);
    #[cfg(any(feature = "libp2p", feature = "iroh"))]
    match &mut config.transport {
        #[cfg(feature = "libp2p")]
        defra_node::TransportConfig::Libp2p(libp2p) => libp2p.sync = sync_tuning(&options),
        #[cfg(feature = "iroh")]
        defra_node::TransportConfig::Iroh(iroh) => iroh.sync = sync_tuning(&options),
        _ => {}
    }
    config.edge = Some(edge);

    // A persistent store opens with the server profile, as FFI always has.
    let mut builder = defra_node::NodeBuilder::default()
        .with_config(config)
        .with_store_profile(defra_node::StoreProfile::Server);
    if let Some(path) = &db_path_opt {
        builder = builder.data_path(path);
    }
    let node = builder.build().await.map_err(|error| error.to_string())?;

    Ok(NodeState {
        database: node.database().clone(),
        background_tasks: node.background_tasks(),
        txn_registry: node.txn_registry().clone(),
        query_runner: node.runner().clone(),
        nac_manager: node.nac_manager().clone(),
        document_acp: node.document_acp(),
        event_bus: node.event_bus().clone(),
        policy_store: Arc::new(PolicyStore::new()),
        local_zanzibar_store: node.local_zanzibar_store().cloned(),
        p2p: node
            .p2p_system()
            .cloned()
            .map(|system| Arc::new(P2PState::new(system))),
        node_identity_did: node
            .node_identity_did()
            .map(str::to_owned)
            .map_or_else(AtomOption::none, AtomOption::some),
        signing_enabled: options.enable_signing != 0,
        #[cfg(feature = "vera")]
        vera_acp: node.vera_acp().cloned(),
        query_limits: node.query_limits(),
        se_encryption_key: AtomOption::none(),
        _node: node,
    })
}

/// Where the node's data lives: `None` keeps it in memory, `Some` is the path
/// of a regolith database.
fn resolve_store(options: &NodeInitOptions) -> Result<Option<String>, String> {
    let backend_name = unsafe { c_str_to_string(options.datastore_backend) }
        .unwrap_or_default()
        .to_lowercase();
    if options.in_memory != 0 || options.db_path.is_null() {
        return Ok(None);
    }
    if !backend_name.is_empty() {
        match defra_core::parse_store_backend(&backend_name) {
            Some(defra_core::StoreBackend::Memory) => return Ok(None),
            Some(defra_core::StoreBackend::Regolith) => {}
            None => {
                return Err(format!(
                    "unknown datastore backend '{}'. Supported: regolith, memory",
                    backend_name
                ));
            }
        }
    }

    let path = unsafe { c_str_to_string(options.db_path) }
        .ok_or_else(|| "db_path is not valid UTF-8".to_string())?;
    Ok(Some(path))
}

fn resolve_embedded_config(options: &NodeInitOptions) -> Result<defra_node::NodeConfig, String> {
    let key = if !options.signing_private_key.is_null() && options.signing_private_key_len > 0 {
        if options.signing_private_key_len > MAX_PRIVATE_KEY_LEN {
            return Err(format!(
                "signing_private_key_len {} exceeds maximum {}",
                options.signing_private_key_len, MAX_PRIVATE_KEY_LEN
            ));
        }
        // SAFETY: `signing_private_key` is non-null (checked above) and
        // `signing_private_key_len` is bounded by MAX_PRIVATE_KEY_LEN.
        // The caller guarantees the pointer is valid for the given length.
        let key_bytes = unsafe {
            std::slice::from_raw_parts(options.signing_private_key, options.signing_private_key_len)
                .to_vec()
        };
        let key_type = unsafe { c_str_to_string(options.signing_key_type) }
            .unwrap_or_else(|| "secp256k1".to_string());
        let signing_key_type: defra_core::signing::SigningKeyType = key_type.parse()?;

        Some(match signing_key_type {
            defra_core::signing::SigningKeyType::Secp256k1 => {
                defra_node::SigningKey::Secp256k1(key_bytes)
            }
            defra_core::signing::SigningKeyType::Secp256r1 => {
                defra_node::SigningKey::Secp256r1(key_bytes)
            }
            defra_core::signing::SigningKeyType::Ed25519 => {
                defra_node::SigningKey::Ed25519(key_bytes)
            }
            defra_core::signing::SigningKeyType::BlsAugV1 => {
                return Err("unsupported signing key type: bls".to_string())
            }
            other => return Err(format!("unsupported signing key type: {}", other)),
        })
    } else {
        None
    };

    let signing = if options.enable_signing != 0 || key.is_some() {
        defra_node::NodeSigning::Enabled { key }
    } else {
        defra_node::NodeSigning::Disabled
    };

    #[cfg(not(feature = "vera"))]
    if !options.vera_grpc_address.is_null() {
        return Err(
            "this build does not include Vera ACP; rebuild with the vera feature".to_string(),
        );
    }
    #[cfg(not(feature = "vera"))]
    let document_acp = defra_node::DocumentAcpConfig::Local;

    #[cfg(feature = "vera")]
    let document_acp = if !options.vera_grpc_address.is_null() {
        let grpc_address = unsafe { c_str_to_string(options.vera_grpc_address) }
            .ok_or_else(|| "vera_grpc_address is not valid UTF-8".to_string())?;
        let comet_rpc_address = unsafe { c_str_to_string(options.vera_comet_rpc_address) }
            .ok_or_else(|| "vera_comet_rpc_address is not valid UTF-8".to_string())?;
        let chain_id = unsafe { c_str_to_string(options.vera_chain_id) }
            .ok_or_else(|| "vera_chain_id is not valid UTF-8".to_string())?;

        if options.vera_signer_key.is_null() || options.vera_signer_key_len == 0 {
            return Err("vera_signer_key is required when Vera is configured".to_string());
        }
        if options.vera_signer_key_len > MAX_PRIVATE_KEY_LEN {
            return Err(format!(
                "vera_signer_key_len {} exceeds maximum {}",
                options.vera_signer_key_len, MAX_PRIVATE_KEY_LEN
            ));
        }

        // SAFETY: `vera_signer_key` is non-null (checked above) and
        // `vera_signer_key_len` is bounded by MAX_PRIVATE_KEY_LEN.
        // The caller guarantees the pointer is valid for the given length.
        let signer_key = unsafe {
            std::slice::from_raw_parts(options.vera_signer_key, options.vera_signer_key_len)
                .to_vec()
        };

        defra_node::DocumentAcpConfig::Vera(defra_node::VeraConfig {
            grpc_address,
            comet_rpc_address,
            chain_id,
            signer_key,
        })
    } else {
        defra_node::DocumentAcpConfig::Local
    };

    Ok(defra_node::NodeConfig {
        peer_keyring: None,
        transport: defra_node::TransportConfig::None,
        edge: None,
        signing,
        document_acp,
        query_limits: query::QueryLimits::default(),
        ..Default::default()
    })
}

/// The sync limits the caller set; a zero leaves the node's default.
#[cfg(any(feature = "libp2p", feature = "iroh"))]
fn sync_tuning(options: &NodeInitOptions) -> defra_node::SyncTuning {
    let mut tuning = defra_node::SyncTuning::default();
    if options.max_concurrent_dag_fetches > 0 {
        tuning.max_concurrent_dag_fetches = options.max_concurrent_dag_fetches;
    }
    if options.max_concurrent_push_tasks > 0 {
        tuning.max_concurrent_push_tasks = options.max_concurrent_push_tasks;
    }
    if options.max_doc_sync_request_doc_ids > 0 {
        tuning.max_doc_sync_request_doc_ids = options.max_doc_sync_request_doc_ids;
    }
    if options.rate_limit_burst > 0 {
        tuning.rate_limit_burst = options.rate_limit_burst;
    }
    if options.rate_limit_rate > 0.0 {
        tuning.rate_limit_rate = options.rate_limit_rate;
    }
    tuning
}

fn with_derived_iroh_key_path(
    transport: defra_node::TransportConfig,
    _db_path: &Option<String>,
) -> defra_node::TransportConfig {
    #[cfg(feature = "iroh")]
    if let defra_node::TransportConfig::Iroh(mut config) = transport {
        // The node keeps one peer key in its store, for either transport; an
        // iroh key file only seeds it.
        config.peer_key = defra_node::PeerKeySource::Store;
        if config.secret_key_path.is_none() {
            if let Some(path) = _db_path {
                config.secret_key_path = Some(std::path::PathBuf::from(format!("{path}.iroh.key")));
            }
        }
        return defra_node::TransportConfig::Iroh(config);
    }

    transport
}

/// Close a DefraDB node and release resources.
#[no_mangle]
pub extern "C" fn node_close(node_ptr: usize) -> FfiResult {
    ffi_entry! {
        use crate::state::{GRAPHQL_SUBSCRIPTIONS, SUBSCRIPTIONS};

        let rt = try_ffi!(crate::helpers::get_rt());

        let removed_subs = SUBSCRIPTIONS.remove_for_node(node_ptr);
        for sub_state in removed_subs {
            let sub_id = sub_state.subscription.id();
            NODES.get(node_ptr, |state| {
                state.event_bus.unsubscribe(sub_id);
            });
        }

        let removed_gql_subs = GRAPHQL_SUBSCRIPTIONS.remove_for_node(node_ptr);
        for sub_state in removed_gql_subs {
            sub_state.service.stop();
            NODES.get(node_ptr, |state| {
                state.event_bus.unsubscribe(sub_state.event_sub_id);
            });
        }

        let state = match NODES.remove(node_ptr) {
            Some(state) => state,
            None => return FfiResult::error(ERR_INVALID_NODE_HANDLE),
        };

        let result = crate::helpers::block_on(rt, async {
            state.background_tasks.shutdown().await;
            if let Some(p2p) = state.p2p.as_ref() {
                p2p.system.shutdown().await;
            }
            state.event_bus.close();
            state
                .database
                .close()
                .await
                .map_err(|error| format!("failed to close database: {}", error))
        });
        drop(state);

        match result {
            Ok(()) => FfiResult::ok(),
            Err(error) => FfiResult::error(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::{CStr, CString};

    use super::*;

    #[test]
    fn test_node_lifecycle() {
        assert!(crate::runtime::init_runtime());

        let result = new_node(NodeInitOptions::default());
        assert_eq!(result.status, 0);
        assert!(result.node_ptr > 0);

        let handle = result.node_ptr;
        assert_eq!(node_close(handle).status, 0);
        assert_eq!(node_close(handle).status, 1);
    }

    #[test]
    fn test_node_close_invalid_handle() {
        assert!(crate::runtime::init_runtime());

        let result = node_close(0);
        assert_eq!(result.status, 1);
        assert!(!result.error.is_null());

        let error = unsafe { std::ffi::CStr::from_ptr(result.error).to_string_lossy() };
        assert!(error.contains("invalid"));

        unsafe { crate::types::defra_free_string(result.error) };
    }

    #[cfg(not(feature = "vera"))]
    #[test]
    fn test_vera_config_rejected_without_feature() {
        assert!(crate::runtime::init_runtime());

        let grpc = CString::new("127.0.0.1:9090").unwrap();
        let comet = CString::new("127.0.0.1:26657").unwrap();
        let chain = CString::new("vera-test").unwrap();
        let signer_key = [1u8; 32];
        let result = new_node(NodeInitOptions {
            vera_grpc_address: grpc.as_ptr(),
            vera_comet_rpc_address: comet.as_ptr(),
            vera_chain_id: chain.as_ptr(),
            vera_signer_key: signer_key.as_ptr(),
            vera_signer_key_len: signer_key.len(),
            ..NodeInitOptions::default()
        });

        assert_eq!(result.status, 1);
        assert!(!result.error.is_null());

        let error = unsafe { CStr::from_ptr(result.error).to_string_lossy().into_owned() };
        assert!(error.contains("Vera"), "unexpected error: {error}");
        assert!(error.contains("vera feature"), "unexpected error: {error}");

        unsafe { crate::types::defra_free_string(result.error) };
    }

    #[cfg(not(feature = "libp2p"))]
    #[test]
    fn test_libp2p_transport_rejected_without_feature() {
        assert!(crate::runtime::init_runtime());

        let listen_addr = CString::new("/ip4/127.0.0.1/tcp/0").unwrap();
        let result = unsafe {
            crate::p2p::new_node_with_p2p(NodeInitOptions::default(), listen_addr.as_ptr())
        };

        assert_eq!(result.status, 1);
        assert!(!result.error.is_null());

        let error = unsafe { CStr::from_ptr(result.error).to_string_lossy().into_owned() };
        assert!(
            error.contains("libp2p transport"),
            "unexpected error: {error}"
        );
        assert!(
            error.contains("libp2p feature"),
            "unexpected error: {error}"
        );

        unsafe { crate::types::defra_free_string(result.error) };
    }

    #[test]
    fn test_multiple_nodes() {
        assert!(crate::runtime::init_runtime());

        let result1 = new_node(NodeInitOptions::default());
        let result2 = new_node(NodeInitOptions::default());

        assert_eq!(result1.status, 0);
        assert_eq!(result2.status, 0);
        assert_ne!(result1.node_ptr, result2.node_ptr);

        assert_eq!(node_close(result1.node_ptr).status, 0);
        assert_eq!(node_close(result2.node_ptr).status, 0);
    }

    #[test]
    fn test_private_key_configures_node_identity_without_enabling_block_signing() {
        assert!(crate::runtime::init_runtime());

        let key = [1u8; 32];
        let key_type = CString::new("secp256k1").unwrap();
        let result = new_node(NodeInitOptions {
            enable_signing: 0,
            signing_key_type: key_type.as_ptr(),
            signing_private_key: key.as_ptr(),
            signing_private_key_len: key.len(),
            ..NodeInitOptions::default()
        });

        assert_eq!(result.status, 0);
        assert_eq!(
            NODES.get(result.node_ptr, |state| (
                state.node_identity_did.is_some(),
                state.signing_enabled,
            )),
            Some((true, false))
        );
        assert_eq!(node_close(result.node_ptr).status, 0);
    }
    #[test]
    fn test_persistent_node_can_reopen_after_close() {
        assert!(crate::runtime::init_runtime());

        let directory = tempfile::tempdir().unwrap();
        let path = CString::new(directory.path().to_string_lossy().as_bytes()).unwrap();
        let backend = CString::new("regolith").unwrap();
        let options = || NodeInitOptions {
            db_path: path.as_ptr(),
            in_memory: 0,
            datastore_backend: backend.as_ptr(),
            ..NodeInitOptions::default()
        };

        let first = new_node(options());
        assert_eq!(first.status, 0);
        assert_eq!(node_close(first.node_ptr).status, 0);

        let second = new_node(options());
        assert_eq!(second.status, 0);
        assert_eq!(node_close(second.node_ptr).status, 0);
    }
    #[cfg(feature = "libp2p")]
    #[test]
    fn test_persistent_p2p_nodes_reopen_after_pending_broadcasts() {
        assert!(crate::runtime::init_runtime());

        let first_directory = tempfile::tempdir().unwrap();
        let second_directory = tempfile::tempdir().unwrap();
        let first_path = CString::new(first_directory.path().to_string_lossy().as_bytes()).unwrap();
        let second_path =
            CString::new(second_directory.path().to_string_lossy().as_bytes()).unwrap();
        let backend = CString::new("regolith").unwrap();
        let listen_addr = CString::new("/ip4/127.0.0.1/tcp/0").unwrap();
        let options = |path: &CString| NodeInitOptions {
            db_path: path.as_ptr(),
            in_memory: 0,
            datastore_backend: backend.as_ptr(),
            ..NodeInitOptions::default()
        };

        let first =
            unsafe { crate::p2p::new_node_with_p2p(options(&first_path), listen_addr.as_ptr()) };
        assert_eq!(first.status, 0);
        let second =
            unsafe { crate::p2p::new_node_with_p2p(options(&second_path), listen_addr.as_ptr()) };
        assert_eq!(second.status, 0);

        let schema = CString::new("type Users { Name: String Age: Int }").unwrap();
        let mutation =
            CString::new(r#"mutation { add_Users(input: {Name: "John", Age: 21}) { _docID } }"#)
                .unwrap();
        for node in [first.node_ptr, second.node_ptr] {
            let result =
                unsafe { crate::schema::add_schema(node, std::ptr::null(), schema.as_ptr()) };
            assert_eq!(result.status, 0);
            unsafe { crate::types::defra_free_string(result.value) };

            let result = unsafe {
                crate::query::exec_request(
                    node,
                    std::ptr::null(),
                    mutation.as_ptr(),
                    std::ptr::null(),
                    std::ptr::null(),
                    std::ptr::null(),
                )
            };
            assert_eq!(result.status, 0);
            unsafe { crate::types::defra_free_string(result.value) };
        }

        let peer_info = unsafe { crate::p2p::p2p_peer_info(second.node_ptr, std::ptr::null()) };
        assert_eq!(peer_info.status, 0);
        let peer_info_json = unsafe { CStr::from_ptr(peer_info.value) }
            .to_string_lossy()
            .to_string();
        unsafe { crate::types::defra_free_string(peer_info.value) };
        let address = serde_json::from_str::<Vec<String>>(&peer_info_json)
            .unwrap()
            .into_iter()
            .next()
            .expect("libp2p should publish a listen address");
        let address = CString::new(address).unwrap();
        let connected =
            unsafe { crate::p2p::p2p_connect(first.node_ptr, std::ptr::null(), address.as_ptr()) };
        assert_eq!(connected.status, 0);

        assert_eq!(node_close(first.node_ptr).status, 0);
        assert_eq!(node_close(second.node_ptr).status, 0);

        for path in [&first_path, &second_path] {
            let reopened =
                unsafe { crate::p2p::new_node_with_p2p(options(path), listen_addr.as_ptr()) };
            if reopened.status != 0 {
                let error = unsafe { CStr::from_ptr(reopened.error) }.to_string_lossy();
                panic!("failed to reopen {}: {error}", path.to_string_lossy());
            }
            assert_eq!(node_close(reopened.node_ptr).status, 0);
        }
    }
}
