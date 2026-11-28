//! A C caller's [`NodeInitOptions`], read into what builds the node.

use std::ffi::c_int;
use std::sync::Arc;

use zeroize::Zeroizing;

use crate::state::{IdentityState, IDENTITIES};
use crate::target::{go_string, non_empty};
use crate::types::NodeInitOptions;

/// Maximum length for private key byte slices passed via FFI.
pub(crate) const MAX_PRIVATE_KEY_LEN: usize = 128;

/// Where the node's store lives.
pub(crate) enum StoreLocation {
    Memory,
    Path(String),
}

/// Everything [`NodeInitOptions`] decides, read before the node is built.
pub(crate) struct NodeSetup {
    pub(crate) store: StoreLocation,
    pub(crate) store_file_size: Option<u64>,
    pub(crate) store_encryption_key: Option<Zeroizing<[u8; 32]>>,
    pub(crate) config: defra_node::NodeConfig,
    /// The identity `identity_ptr` names, kept until the node holds its key,
    /// so freeing the handle while the node starts cannot take the key away.
    pub(crate) identity: Option<Arc<IdentityState>>,
    pub(crate) signing_enabled: bool,
    pub(crate) searchable_encryption_key: Option<Zeroizing<Vec<u8>>>,
    pub(crate) peer_key: Option<Zeroizing<Vec<u8>>>,
}

/// Reads `options`.
///
/// # Safety
///
/// Every non-null pointer in `options` must be valid: strings
/// null-terminated, byte buffers at least their stated length.
pub(crate) unsafe fn read(options: &NodeInitOptions) -> Result<NodeSetup, String> {
    reject_unsupported(options)?;

    let store_type = go_string(options.store_type).to_lowercase();
    let db_path = non_empty(options.db_path);
    let store = match (store_type.as_str(), db_path) {
        ("memory", _) | (_, None) => StoreLocation::Memory,
        _ if options.in_memory != 0 => StoreLocation::Memory,
        ("", Some(path)) => StoreLocation::Path(path),
        (other, Some(path)) => match defra_core::parse_store_backend(other) {
            Some(defra_core::StoreBackend::Memory) => StoreLocation::Memory,
            Some(defra_core::StoreBackend::Regolith) => StoreLocation::Path(path),
            None => {
                return Err(format!(
                    "unknown store type '{}'. Supported: regolith, memory",
                    other
                ))
            }
        },
    };

    let identity = match options.identity_ptr {
        0 => None,
        handle => Some(
            IDENTITIES
                .get(handle)
                .ok_or_else(|| "identity_ptr is not an identity handle".to_string())?,
        ),
    };
    let signing = match &identity {
        Some(identity) => defra_node::NodeSigning::RegisteredIdentity {
            did: identity.did.clone(),
        },
        None if options.disable_signing == 0 => defra_node::NodeSigning::Enabled { key: None },
        None => defra_node::NodeSigning::Disabled,
    };

    let config = defra_node::NodeConfig {
        signing,
        document_acp: document_acp(options)?,
        max_txn_retries: positive_int(options.max_transaction_retries)
            .map(|retries| retries as u32),
        chunk_size: positive_int(options.chunk_size).map(|size| size as usize),
        node_acp_enabled: options.enable_node_acp != 0,
        replication_filter: crate::replication_filter::from_options(options),
        ..Default::default()
    };

    Ok(NodeSetup {
        store,
        store_file_size: (options.store_file_size > 0).then_some(options.store_file_size as u64),
        store_encryption_key: key_bytes(
            options.store_encryption_key,
            options.store_encryption_key_len,
            "store_encryption_key",
        )?
        .map(|key| {
            let key: [u8; 32] = key
                .as_slice()
                .try_into()
                .map_err(|_| format!("store_encryption_key must be 32 bytes, got {}", key.len()))?;
            Ok::<_, String>(Zeroizing::new(key))
        })
        .transpose()?,
        config,
        identity,
        signing_enabled: options.disable_signing == 0,
        searchable_encryption_key: key_bytes(
            options.searchable_encryption_key,
            options.searchable_encryption_key_len,
            "searchable_encryption_key",
        )?,
        peer_key: key_bytes(
            options.p2p_private_key,
            options.p2p_private_key_len,
            "p2p_private_key",
        )?,
    })
}

/// Go options this implementation does not support yet: set, they fail the
/// call rather than leave the caller believing they took effect.
///
/// # Safety
///
/// As for [`read`].
unsafe fn reject_unsupported(options: &NodeInitOptions) -> Result<(), String> {
    let http_set = non_empty(options.http_address).is_some()
        || non_empty(options.http_allowed_origins).is_some()
        || non_empty(options.tls_cert_path).is_some()
        || non_empty(options.tls_key_path).is_some()
        || options.http_read_timeout_ms != 0
        || options.http_write_timeout_ms != 0
        || options.http_idle_timeout_ms != 0;
    let unsupported = [
        (
            http_set,
            "the HTTP API options: an embedded node does not serve the HTTP API yet",
        ),
        (options.lens_pool_size != 0, "lens_pool_size"),
        (
            options.enable_clear_backoff_on_retry != 0,
            "enable_clear_backoff_on_retry",
        ),
        (
            non_empty(options.document_acp_path).is_some(),
            "document_acp_path: document ACP lives in the node's store",
        ),
        (
            non_empty(options.node_acp_path).is_some(),
            "node_acp_path: node ACP lives in the node's store",
        ),
        (
            // Older callers can still pass empty C strings in these slots.
            non_empty(options._reserved_acp0).is_some()
                || non_empty(options._reserved_acp1).is_some()
                || non_empty(options._reserved_acp2).is_some()
                || non_empty(options._reserved_acp3).is_some()
                || !options._reserved_acp4.is_null()
                || options._reserved_acp5 != 0,
            REMOTE_DOCUMENT_ACP,
        ),
    ];
    match unsupported.iter().find(|(set, _)| *set) {
        Some((_, what)) => Err(format!("not supported yet: {}", what)),
        None => Ok(()),
    }
}

/// The document ACP `document_acp_type` names.
///
/// # Safety
///
/// As for [`read`].
unsafe fn document_acp(options: &NodeInitOptions) -> Result<defra_node::DocumentAcpConfig, String> {
    match go_string(options.document_acp_type).to_lowercase().as_str() {
        "" | "local" => Ok(defra_node::DocumentAcpConfig::Local),
        "remote" => Err(format!("not supported yet: {REMOTE_DOCUMENT_ACP}")),
        "none" => Err("not supported yet: document_acp_type \"none\"".to_string()),
        other => Err(format!(
            "unknown document_acp_type '{}'. Supported: local",
            other
        )),
    }
}

const REMOTE_DOCUMENT_ACP: &str = "remote document ACP (use document_acp_type \"local\")";

/// A key the caller passed as a buffer and its length, or `None` for none.
///
/// # Safety
///
/// `ptr` must be null or valid for `len` bytes.
pub(crate) unsafe fn key_bytes(
    ptr: *const u8,
    len: c_int,
    name: &str,
) -> Result<Option<Zeroizing<Vec<u8>>>, String> {
    if ptr.is_null() || len <= 0 {
        return Ok(None);
    }
    let len = len as usize;
    if len > MAX_PRIVATE_KEY_LEN {
        return Err(format!(
            "{}_len {} exceeds maximum {}",
            name, len, MAX_PRIVATE_KEY_LEN
        ));
    }
    // SAFETY: non-null and bounded above; the caller guarantees the buffer
    // is valid for its stated length.
    Ok(Some(Zeroizing::new(
        std::slice::from_raw_parts(ptr, len).to_vec(),
    )))
}

fn positive_int(value: c_int) -> Option<c_int> {
    (value > 0).then_some(value)
}
