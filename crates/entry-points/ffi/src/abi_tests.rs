//! The C signatures hosts bind by hand (the Swift and Kotlin wrappers): a
//! change to one breaks them at link or call time, so it must fail here
//! first.

use std::ffi::{c_char, c_int};

use crate::types::{CollectionOptions, DefraResult};

type NodeCall = extern "C" fn(usize, usize) -> DefraResult;
type AddressCall = unsafe extern "C" fn(usize, *const c_char, usize) -> DefraResult;

#[test]
fn the_mobile_and_peer_calls_take_a_handle_and_an_identity() {
    let node_calls: [NodeCall; 5] = [
        crate::p2p::defra_mobile_shareable_address,
        crate::p2p::defra_mobile_sync_status,
        crate::p2p::defra_mobile_notify_network_change,
        crate::p2p::defra_p2p_info,
        crate::p2p::defra_p2p_peer_list,
    ];
    let address_calls: [AddressCall; 3] = [
        crate::p2p::defra_p2p_connect,
        crate::p2p::defra_p2p_disconnect,
        crate::p2p::defra_mobile_peer_details,
    ];
    assert_eq!(node_calls.len() + address_calls.len(), 8);
}

#[test]
fn the_document_and_query_calls_keep_their_shape() {
    let _add: unsafe extern "C" fn(
        usize,
        *const c_char,
        c_int,
        *const c_char,
        CollectionOptions,
        usize,
    ) -> DefraResult = crate::document::defra_document_add;
    let _query: unsafe extern "C" fn(
        usize,
        *const c_char,
        usize,
        *const c_char,
        *const c_char,
    ) -> DefraResult = crate::query::defra_query_execute;
    let _commit: extern "C" fn(usize) -> DefraResult = crate::txn::defra_transaction_commit;
    let _discard: extern "C" fn(usize) = crate::txn::defra_transaction_discard;
    let _free: extern "C" fn(usize) = crate::identities::defra_identity_free;
}
