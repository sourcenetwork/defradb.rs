//! The C signatures hosts bind by hand (the Swift and Kotlin wrappers): a
//! change to one breaks them at link or call time, so it must fail here
//! first. Each call that takes options is exported with its options by
//! pointer and their size; the size is what lets a struct grow.

use std::ffi::c_char;

use crate::api::document::DefraDocumentAddOptions;
use crate::api::identity::{DefraIdentityBearerOptions, DefraRemoteIdentityOptions};
use crate::api::p2p::DefraP2POptions;
use crate::api::query::DefraQueryOptions;
use crate::types::{DefraNewIdentityResult, DefraResult};

type NodeCall = unsafe extern "C" fn(usize, *const DefraP2POptions, u32) -> DefraResult;
type AddressCall =
    unsafe extern "C" fn(usize, *const c_char, *const DefraP2POptions, u32) -> DefraResult;

#[test]
fn the_mobile_and_peer_calls_take_a_handle_and_their_options() {
    let node_calls: [NodeCall; 5] = [
        crate::api::p2p::defra_mobile_shareable_address_sized,
        crate::api::p2p::defra_mobile_sync_status_sized,
        crate::api::p2p::defra_mobile_notify_network_change_sized,
        crate::api::p2p::defra_p2p_info_sized,
        crate::api::p2p::defra_p2p_peer_list_sized,
    ];
    let address_calls: [AddressCall; 3] = [
        crate::api::p2p::defra_p2p_connect_sized,
        crate::api::p2p::defra_p2p_disconnect_sized,
        crate::api::p2p::defra_mobile_peer_details_sized,
    ];
    assert_eq!(node_calls.len() + address_calls.len(), 8);
}

#[test]
fn the_document_query_and_identity_calls_keep_their_shape() {
    let _add: unsafe extern "C" fn(
        usize,
        *const c_char,
        *const DefraDocumentAddOptions,
        u32,
    ) -> DefraResult = crate::api::document::defra_document_add_sized;
    let _query: unsafe extern "C" fn(
        usize,
        *const c_char,
        *const DefraQueryOptions,
        u32,
    ) -> DefraResult = crate::api::query::defra_query_execute_sized;
    let _remote: unsafe extern "C" fn(
        *const c_char,
        *const DefraRemoteIdentityOptions,
        u32,
    ) -> DefraNewIdentityResult = crate::api::identity::defra_identity_new_remote_sized;
    let _bearer: unsafe extern "C" fn(
        *const c_char,
        *const c_char,
        *const DefraIdentityBearerOptions,
        u32,
    ) -> DefraNewIdentityResult = crate::api::identity::defra_identity_new_from_bearer_token_sized;
    let _commit: extern "C" fn(usize) -> DefraResult = crate::txn::defra_transaction_commit;
    let _discard: extern "C" fn(usize) = crate::txn::defra_transaction_discard;
    let _free: extern "C" fn(usize) = crate::identities::defra_identity_free;
}
