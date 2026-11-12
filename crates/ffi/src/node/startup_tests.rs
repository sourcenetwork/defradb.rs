//! Which stores and features a node starts with, its identity without
//! signing, and how its handle ends.

use std::ffi::CStr;

use super::*;
use crate::identities::{defra_identity_free, defra_identity_new};
use crate::state::NODES;
use crate::test_support::{close, cstr, err, local_options, node, node_with};
use crate::types::defra_new_node_result_free;

fn refused(options: NodeInitOptions) -> String {
    let mut result = unsafe { defra_node_new(options) };
    assert_eq!(result.status, 1, "the node must not start");
    let error = unsafe { CStr::from_ptr(result.error) }
        .to_string_lossy()
        .into_owned();
    unsafe { defra_new_node_result_free(&mut result) };
    error
}

#[test]
fn the_stores_regolith_replaced_are_refused() {
    let directory = tempfile::tempdir().unwrap();
    let path = cstr(&directory.path().to_string_lossy());
    for name in ["lark", "redb", "fjall", "rocksdb", "leveldb"] {
        let store_type = cstr(name);
        let error = refused(NodeInitOptions {
            db_path: path.as_ptr(),
            store_type: store_type.as_ptr(),
            ..local_options()
        });
        assert!(error.contains(name), "{name}: {error}");
    }
}

#[test]
fn regolith_badger_and_memory_start_a_node() {
    for name in ["regolith", "badger", "memory", ""] {
        let directory = tempfile::tempdir().unwrap();
        let path = cstr(&directory.path().to_string_lossy());
        let store_type = cstr(name);
        let node = node_with(NodeInitOptions {
            db_path: path.as_ptr(),
            store_type: store_type.as_ptr(),
            ..local_options()
        });
        close(node);
    }
}

#[test]
fn two_nodes_run_side_by_side_under_their_own_handles() {
    let first = node();
    let second = node();
    assert_ne!(first, second);
    close(first);
    close(second);
}

#[test]
fn a_closed_node_cannot_be_closed_again() {
    let node = node();
    close(node);
    let error = err(defra_node_close(node));
    assert!(error.contains("invalid"), "{error}");
}

#[test]
fn an_identity_without_signing_names_the_node_and_signs_nothing() {
    let identity = unsafe { defra_identity_new(std::ptr::null()) }.identity_ptr;
    let node = node_with(NodeInitOptions {
        identity_ptr: identity,
        disable_signing: 1,
        ..local_options()
    });
    assert_eq!(
        NODES.get(node, |state| (
            state.identity_did().is_some(),
            state.signing_enabled
        )),
        Some((true, false))
    );
    close(node);
    defra_identity_free(identity);
}

#[test]
fn an_identity_option_that_is_not_an_identity_handle_fails() {
    let error = refused(NodeInitOptions {
        identity_ptr: usize::MAX,
        ..local_options()
    });
    assert!(error.contains("identity_ptr"), "{error}");
}

#[cfg(not(feature = "vera"))]
#[test]
fn remote_document_acp_needs_the_vera_feature() {
    let remote = cstr("remote");
    let error = refused(NodeInitOptions {
        document_acp_type: remote.as_ptr(),
        ..local_options()
    });
    assert!(error.contains("vera feature"), "{error}");
}

#[cfg(feature = "vera")]
#[test]
fn remote_document_acp_needs_a_signer_key() {
    let remote = cstr("remote");
    let error = refused(NodeInitOptions {
        document_acp_type: remote.as_ptr(),
        ..local_options()
    });
    assert!(error.contains("vera_signer_key"), "{error}");
}

#[cfg(not(feature = "libp2p"))]
#[test]
fn p2p_without_a_transport_feature_fails() {
    let listen = cstr("/ip4/127.0.0.1/tcp/0");
    let error = refused(NodeInitOptions {
        disable_p2p: 0,
        listening_addresses: listen.as_ptr(),
        ..local_options()
    });
    assert!(error.contains("libp2p feature"), "{error}");
}

#[cfg(not(feature = "iroh"))]
#[test]
fn the_iroh_transport_needs_the_iroh_feature() {
    let iroh = cstr("iroh");
    let error = refused(NodeInitOptions {
        disable_p2p: 0,
        p2p_transport: iroh.as_ptr(),
        ..local_options()
    });
    assert!(error.contains("iroh"), "{error}");
}
