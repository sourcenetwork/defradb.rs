//! What P2P setup hands back, and the SE key tee it shares.
//!
//! Not part of the public `defra_node` API.

use std::sync::Arc;

use crate::managed_p2p::ManagedP2PSystem;
use defra_p2p_adapter::ReplicatorPushOptions;

pub(crate) type WireKmsCallback = Box<dyn FnOnce(Arc<dyn kms::KmsService>) + Send>;

pub(crate) struct P2PSetup {
    pub system: Arc<ManagedP2PSystem>,
    pub mutator: Arc<dyn query::DocMutator>,
    /// Forwards committed `/tx` writes to P2P peers; mirrors what the CLI
    /// `P2PSetup` exposes. Without this, transactional writes commit locally
    /// but never replicate.
    pub txn_broadcaster: Arc<dyn db::event::emission::TxnBroadcaster>,
    /// Type-erased KMS transport for this node's P2P system. node.rs adds it
    /// to the DefraKms transports list and installs the serve handler.
    pub kms_transport: Arc<dyn kms::KeyTransport>,
    /// This node's transport-level peer id (stringified). node.rs binds it
    /// into the KMS so served ECIES replies carry the correct AAD peer id.
    pub local_peer_id: String,
    /// Defers wiring the late-built KMS into the inner merge handler. NAC
    /// isn't available when the P2P system is created, so the KMS is built
    /// later in node.rs.
    pub wire_kms: Option<WireKmsCallback>,
    /// SE remote query transport (owner-queries-replicator, #976). Lets this
    /// node act as an SE query OWNER, fanning `encrypted_<Collection>`
    /// queries to replicators. The SE key is read lazily because it's
    /// provisioned at runtime via `set_se_options`.
    pub se_transport: Option<Arc<dyn query::SeQueryTransport>>,
    /// Inbound management-channel serve deps, read lazily by the event loop and
    /// populated by node.rs once the controller (`P2POperations`) and NAC
    /// manager are built. The event loop drops manage requests until then.
    pub manage_hooks: defra_p2p_adapter::manage::hooks::ManageHooksCell,
    /// The `P2POperations` controller (the `adapter`) bound into `manage_hooks`
    /// after the NAC manager exists. node.rs uses it as `hooks.ops`.
    pub manage_controller: Arc<dyn defra_p2p_adapter::P2POperations>,
    /// Requester-side manage correlators (mutating + query). The event-loop
    /// clones deliver inbound replies; these clones are for the requester API
    /// (Task 6.3) and are bound into `manage_hooks` so requester and event loop
    /// agree on message_id correlation.
    pub manage_correlator: p2p::ManageCorrelator,
    pub manage_query_correlator: p2p::ManageQueryCorrelator,
    /// Dials the configured bootstrap peers in the background. Called once
    /// node access control is wired, so no peer connects before it.
    pub connect_bootstrap: Option<Box<dyn FnOnce() + Send>>,
}

/// Tee the SE key material from runtime `set_se_options` into the lazy handle
/// read by the owner/querier transport. Skips non-32-byte keys (#976).
pub(crate) fn tee_se_key(handle: &db::merge::SeKeyHandle, options: &ReplicatorPushOptions) {
    match &options.se_encryption_key {
        Some(key_bytes) => match <[u8; 32]>::try_from(key_bytes.as_slice()) {
            Ok(key) => db::merge::store_se_key(
                handle,
                Some(db::merge::SeKeyMaterial::new(
                    key,
                    options.se_identity_pubkey.clone(),
                )),
            ),
            Err(_) => {
                tracing::warn!(
                    len = key_bytes.len(),
                    "SE key from set_se_options is not 32 bytes; skipping owner-transport tee"
                );
            }
        },
        None => db::merge::store_se_key(handle, None),
    }
}
