//! A document call's `enable_signing`: 0 follows the node, -1 leaves the
//! call's blocks unsigned even on a signing node, 1 signs even on a node that
//! does not (#1600).

use std::ffi::c_int;
use std::ptr;

use serde_json::Value;

use super::{defra_document_add, defra_document_save_many};
use crate::identities::{defra_identity_free, defra_identity_new};
use crate::test_support::{
    add_collection, close, cstr, local_options, node_with, ok, ok_json, query,
};
use crate::txn::{defra_transaction_commit, defra_transaction_create};
use crate::types::{CollectionOptions, NodeInitOptions};

/// An in-memory node with a `Widget` collection, signing by default when
/// `signing` is set. Each node generates its own identity, so tests running
/// at once never share one.
fn widgets(signing: bool) -> usize {
    let node = node_with(NodeInitOptions {
        disable_signing: c_int::from(!signing),
        ..local_options()
    });
    add_collection(node, "type Widget { name: String }");
    node
}

/// Adds a widget through `handle` (a node or a transaction) with the call's
/// `enable_signing`, as `identity`, and answers with its document ID.
fn add_widget(handle: usize, enable_signing: c_int, identity: usize) -> String {
    let name = cstr("Widget");
    let json = cstr(r#"{"name": "w"}"#);
    let options = CollectionOptions {
        name: name.as_ptr(),
        enable_signing,
        ..CollectionOptions::default()
    };
    let ids = ok_json(unsafe {
        defra_document_add(handle, json.as_ptr(), 0, ptr::null(), options, identity)
    });
    ids[0].as_str().expect("add answers with an ID").to_string()
}

/// The `signature` of every block of `doc_id`.
fn signatures(node: usize, doc_id: &str) -> Vec<Value> {
    let response = query(
        node,
        &format!(
            r#"{{ _commits(docID: "{doc_id}") {{ cid signature {{ type identity value }} }} }}"#
        ),
    );
    let rows = response["data"]["_commits"]
        .as_array()
        .unwrap_or_else(|| panic!("no commits in {response}"))
        .clone();
    assert!(!rows.is_empty(), "the document has blocks");
    rows.into_iter()
        .map(|row| row["signature"].clone())
        .collect()
}

fn all_signed(signatures: &[Value]) -> bool {
    signatures.iter().all(|signature| !signature.is_null())
}

fn none_signed(signatures: &[Value]) -> bool {
    signatures.iter().all(Value::is_null)
}

#[test]
fn the_node_default_signs_on_a_signing_node() {
    let node = widgets(true);
    let doc_id = add_widget(node, 0, 0);
    let signatures = signatures(node, &doc_id);
    assert!(all_signed(&signatures), "{signatures:?}");
    close(node);
}

#[test]
fn opting_out_leaves_the_blocks_unsigned_on_a_signing_node() {
    let node = widgets(true);
    let doc_id = add_widget(node, -1, 0);
    let signatures = signatures(node, &doc_id);
    assert!(none_signed(&signatures), "{signatures:?}");
    close(node);
}

/// save_many runs its documents as one typed write, which must keep the
/// call's opt-out rather than sign as the node does.
#[test]
fn opting_out_of_a_save_many_leaves_the_blocks_unsigned_on_a_signing_node() {
    let node = widgets(true);
    let name = cstr("Widget");
    let json = cstr(r#"[{"name": "w"}]"#);
    let options = CollectionOptions {
        name: name.as_ptr(),
        enable_signing: -1,
        ..CollectionOptions::default()
    };
    let ids = ok_json(unsafe {
        defra_document_save_many(node, json.as_ptr(), 0, ptr::null(), options, 0)
    });
    let doc_id = ids[0].as_str().expect("save_many answers with an ID");

    let signatures = signatures(node, doc_id);
    assert!(none_signed(&signatures), "{signatures:?}");
    close(node);
}

#[test]
fn opting_out_inside_a_transaction_leaves_the_blocks_unsigned() {
    let node = widgets(true);
    let txn = defra_transaction_create(node, 0);
    assert_eq!(txn.status, 0, "defra_transaction_create must succeed");

    let doc_id = add_widget(txn.txn_ptr, -1, 0);
    ok(defra_transaction_commit(txn.txn_ptr));

    let signatures = signatures(node, &doc_id);
    assert!(none_signed(&signatures), "{signatures:?}");
    close(node);
}

#[test]
fn the_node_default_leaves_the_blocks_unsigned_on_a_node_that_does_not_sign() {
    let node = widgets(false);
    let doc_id = add_widget(node, 0, 0);
    let signatures = signatures(node, &doc_id);
    assert!(none_signed(&signatures), "{signatures:?}");
    close(node);
}

#[test]
fn opting_in_signs_as_the_callers_identity_on_a_node_that_does_not_sign() {
    let node = widgets(false);
    let identity = unsafe { defra_identity_new(ptr::null()) }.identity_ptr;
    let did = ok(crate::identities::defra_identity_did(identity));

    let doc_id = add_widget(node, 1, identity);
    let signatures = signatures(node, &doc_id);
    assert!(all_signed(&signatures), "{signatures:?}");
    assert!(
        signatures.iter().all(|signature| signature["identity"]
            .as_str()
            .is_some_and(|id| !id.is_empty())),
        "every block names its signer ({did}): {signatures:?}"
    );

    defra_identity_free(identity);
    close(node);
}
