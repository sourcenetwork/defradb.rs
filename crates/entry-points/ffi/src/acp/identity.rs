//! Identities this implementation adds to Go's: one a bearer token stands
//! for, and setting a node's identity after it starts.

use std::ffi::c_char;

use zeroize::Zeroizing;

use crate::state::{IdentityState, NodeIdentity, IDENTITIES, NODES};
use crate::target::{non_empty, resolve_identity_state, resolve_node};
use crate::types::{DefraNewIdentityResult, DefraResult};
use crate::{ffi_entry, try_ffi};

/// Creates an identity for the logical DID `did`, acting by the bearer token
/// the host provides instead of a key of its own: the token may be signed by
/// a device-bound key authorized to act on behalf of `did`. Answers with the
/// identity's handle.
///
/// The token is presented only by requests made with this handle, and goes
/// when the handle is freed; another handle for the same DID presents its
/// own. The identity has no key, so on a signing node its blocks are signed
/// with the node's key, as Go signs for an identity without a private key.
///
/// # Safety
///
/// `did` and `bearer_token` must be null or valid null-terminated strings.
#[no_mangle]
pub unsafe extern "C" fn defra_identity_new_from_bearer_token(
    did: *const c_char,
    bearer_token: *const c_char,
) -> DefraNewIdentityResult {
    ffi_entry! {
        let Some(did) = non_empty(did) else {
            return DefraNewIdentityResult::error("invalid did parameter");
        };
        let Some(token) = non_empty(bearer_token) else {
            return DefraNewIdentityResult::error("invalid bearer_token parameter");
        };
        if let Err(error) = identity::Did::new(did.clone()) {
            return DefraNewIdentityResult::error(format!("invalid DID: {}", error));
        }

        DefraNewIdentityResult::success(IDENTITIES.insert(IdentityState {
            did,
            key_type: None,
            private_key: None,
            public_key_hex: String::new(),
            hold: None,
            bearer_token: Some(Zeroizing::new(token)),
        }))
    }
}

/// Sets the node's identity, which signs the node's blocks when signing is
/// enabled, to the one `identity_ptr` names; 0 clears it.
#[no_mangle]
pub extern "C" fn defra_node_set_identity(node_ptr: usize, identity_ptr: usize) -> DefraResult {
    ffi_entry! {
        let node = try_ffi!(resolve_node(node_ptr));
        // The state, not just its DID: while it lives its key stays in the
        // signing store, so the node's hold finds it even if the handle is
        // freed meanwhile.
        let identity = try_ffi!(resolve_identity_state(identity_ptr));
        let updated = NODES.get(node, |state| {
            let replaced = state.node_identity.swap(
                identity
                    .as_ref()
                    .map(|identity| NodeIdentity::hold(identity.did.clone())),
            );
            if let Some(identity) = &*replaced {
                identity.release();
            }
            state.sync_replicator_push_options()
        });
        match updated {
            Some(Ok(())) => DefraResult::ok(),
            Some(Err(error)) => DefraResult::error(error),
            None => DefraResult::error(crate::target::ERR_INVALID_HANDLE),
        }
    }
}
