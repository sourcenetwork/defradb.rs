//! FFI batch signing functions.
//!
//! A caller starts a batch, makes its writes, then signs every block they
//! made at once, keyed by the signing identity's public key.

use crate::ffi_entry;
use crate::helpers::get_rt;
use crate::state::NODES;
use crate::target::resolve_identity;
use crate::try_ffi;
use crate::types::DefraResult;

/// Starts (or restarts) collecting the blocks the identity `identity_ptr`
/// names (0 for the node's) signs.
#[no_mangle]
pub extern "C" fn defra_batch_start(node_ptr: usize, identity_ptr: usize) -> DefraResult {
    ffi_entry! {
        let _rt = try_ffi!(get_rt());
        let identity_str = try_ffi!(resolve_identity(identity_ptr));

        let (node_did, signing_enabled) = NODES
            .get(node_ptr, |state| (state.identity_did(), state.signing_enabled))
            .unwrap_or((None, false));

        let signing = defra_core::signing::resolve_signing_config_with_flag(
            identity_str.as_deref(),
            node_did.as_deref(),
            signing_enabled,
        );

        match signing {
            Some(config) => {
                let key = config.public_key_hex.clone();
                defra_core::batch_signing::batch_start(&key);
                DefraResult::ok()
            }
            None => DefraResult::error("no signing identity available"),
        }
    }
}

/// Sign all collected batch CIDs and return the batch signature as JSON.
///
/// Returns JSON with fields: sig_type, identity, value (hex), merkle_root (hex), cid_count.
///
/// The identity is the one `defra_batch_start` was given.
#[no_mangle]
pub extern "C" fn defra_batch_sign(node_ptr: usize, identity_ptr: usize) -> DefraResult {
    ffi_entry! {
        let _rt = try_ffi!(get_rt());
        let identity_str = try_ffi!(resolve_identity(identity_ptr));

        let (node_did, signing_enabled) = NODES
            .get(node_ptr, |state| (state.identity_did(), state.signing_enabled))
            .unwrap_or((None, false));

        let signing = defra_core::signing::resolve_signing_config_with_flag(
            identity_str.as_deref(),
            node_did.as_deref(),
            signing_enabled,
        );

        let config = match signing {
            Some(c) => c,
            None => return DefraResult::error("no signing identity available"),
        };

        let key = config.public_key_hex.clone();
        let cids = match defra_core::batch_signing::batch_take_cids(&key) {
            Some(c) => c,
            None => return DefraResult::error("no batch session active"),
        };

        if cids.is_empty() {
            return DefraResult::error("batch has no CIDs");
        }

        match crypto::batch::sign_batch(&cids, &config) {
            Ok(sig) => match serde_json::to_string(&sig) {
                Ok(json) => DefraResult::success(json),
                Err(e) => DefraResult::error(format!("failed to serialize batch signature: {}", e)),
            },
            Err(e) => DefraResult::error(format!("batch sign failed: {}", e)),
        }
    }
}
