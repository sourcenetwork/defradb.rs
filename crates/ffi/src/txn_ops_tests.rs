//! Index, encrypted index and view calls on a transaction handle run in that
//! transaction: what they change is seen in it, unseen outside it until it
//! commits, and gone if it is discarded, as Go's transactions keep them.
//! Truncate, which Go does not run in a transaction, applies at once.

use std::ptr;

use serde_json::{json, Value};

use crate::collection::defra_collection_truncate;
use crate::encrypted_index::{defra_encrypted_index_list, defra_encrypted_index_new};
use crate::index::{defra_index_delete, defra_index_list, defra_index_new};
use crate::test_support::{add_collection, close, cstr, node, ok, ok_json, query, Named};
use crate::txn::{defra_transaction_commit, defra_transaction_create, defra_transaction_discard};
use crate::view::defra_view_add;

/// A node holding `User { name }` with one document.
fn users() -> usize {
    let node = node();
    add_collection(node, "type User { name: String }");
    query(
        node,
        r#"mutation { add_User(input: {name: "A"}) { _docID } }"#,
    );
    node
}

fn begin(node: usize) -> usize {
    defra_transaction_create(node, 0).txn_ptr
}

/// The names of the indexes of `User` on `handle`.
fn index_names(handle: usize) -> Vec<String> {
    let user = Named::new("User");
    let listed = ok_json(unsafe { defra_index_list(handle, user.options(), 0) });
    listed
        .as_array()
        .unwrap_or_else(|| panic!("an index list: {listed}"))
        .iter()
        .map(|index| index["Description"]["Name"].as_str().unwrap().to_owned())
        .collect()
}

fn create_index(handle: usize, name: &str) {
    let user = Named::new("User");
    let (name, fields) = (cstr(name), cstr("name"));
    ok(unsafe {
        defra_index_new(
            handle,
            name.as_ptr(),
            fields.as_ptr(),
            0,
            ptr::null(),
            user.options(),
            0,
        )
    });
}

#[test]
fn an_index_created_in_a_transaction_is_seen_only_in_it_until_it_commits() {
    let node = users();
    let txn = begin(node);

    create_index(txn, "by_name");
    assert_eq!(index_names(txn), ["by_name"], "the transaction sees it");
    assert!(index_names(node).is_empty(), "outside, nothing yet");

    let all = ok_json(unsafe { defra_index_list(txn, Default::default(), 0) });
    assert!(
        all.get("User").is_some(),
        "every collection's, by name: {all}"
    );

    ok(defra_transaction_commit(txn));
    assert_eq!(index_names(node), ["by_name"], "committed");
    close(node);
}

#[test]
fn an_index_deleted_in_a_discarded_transaction_stays() {
    let node = users();
    create_index(node, "by_name");
    let txn = begin(node);

    let (user, name) = (Named::new("User"), cstr("by_name"));
    ok(unsafe { defra_index_delete(txn, name.as_ptr(), user.options(), 0) });
    assert!(index_names(txn).is_empty(), "deleted in the transaction");
    assert_eq!(index_names(node), ["by_name"], "still there outside");

    defra_transaction_discard(txn);
    assert_eq!(index_names(node), ["by_name"]);
    close(node);
}

#[test]
fn an_index_created_in_a_transaction_covers_its_own_documents() {
    let node = node();
    add_collection(node, "type User { name: String }");
    let txn = begin(node);
    query(
        txn,
        r#"mutation { add_User(input: {name: "B"}) { _docID } }"#,
    );

    create_index(txn, "by_name");
    ok(defra_transaction_commit(txn));

    let found = query(node, r#"{ User(filter: {name: {_eq: "B"}}) { name } }"#);
    assert_eq!(found["data"]["User"], json!([{"name": "B"}]), "{found}");
    close(node);
}

#[test]
fn an_encrypted_index_added_in_a_transaction_is_seen_only_in_it_until_it_commits() {
    let node = users();
    let txn = begin(node);
    let (collection, field) = (cstr("User"), cstr("name"));

    ok(unsafe {
        defra_encrypted_index_new(txn, collection.as_ptr(), field.as_ptr(), ptr::null(), 0)
    });
    let count = |handle: usize| {
        let listed = ok_json(unsafe { defra_encrypted_index_list(handle, collection.as_ptr(), 0) });
        listed.as_array().map(Vec::len).unwrap_or_default()
    };
    assert_eq!(count(txn), 1, "the transaction sees it");
    assert_eq!(count(node), 0, "outside, nothing yet");

    ok(defra_transaction_commit(txn));
    assert_eq!(count(node), 1, "committed");
    close(node);
}

fn user_names(handle: usize) -> Value {
    query(handle, "{ User { name } }")["data"]["User"].clone()
}

/// Go's `Collection.Truncate` is not transactional: on a transaction handle
/// it is accepted and applies at once, kept even if the transaction is
/// discarded.
#[test]
fn a_truncate_on_a_transaction_applies_at_once() {
    let node = users();
    let txn = begin(node);
    let user = Named::new("User");

    ok(unsafe { defra_collection_truncate(txn, user.options(), 0) });
    assert_eq!(user_names(node), json!([]), "applied at once");

    defra_transaction_discard(txn);
    assert_eq!(
        user_names(node),
        json!([]),
        "a discard does not bring it back"
    );
    close(node);
}

#[test]
fn a_view_added_in_a_transaction_exists_only_once_it_commits() {
    let node = users();
    let txn = begin(node);
    let (view_query, sdl) = (
        cstr("User { name }"),
        cstr("type UserView @materialized(if: false) { name: String }"),
    );

    let created =
        ok_json(unsafe { defra_view_add(txn, view_query.as_ptr(), sdl.as_ptr(), ptr::null(), 0) });
    assert_eq!(created[0]["Name"], "UserView");
    let view = |handle: usize| query(handle, "{ UserView { name } }");
    assert!(view(node).get("errors").is_some(), "outside, no view yet");

    ok(defra_transaction_commit(txn));
    assert_eq!(view(node)["data"]["UserView"], json!([{"name": "A"}]));
    close(node);
}
