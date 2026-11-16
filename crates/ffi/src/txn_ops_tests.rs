//! Index, encrypted index and view calls on a transaction handle run in that
//! transaction: what they change is seen in it, unseen outside it until it
//! commits, and gone if it is discarded, as Go's transactions keep them.
//! Truncate, which Go does not run in a transaction, applies at once, and a
//! filtered one is refused.

use std::ptr;
use std::time::Duration;

use defra_core::ActionStatus;
use serde_json::{json, Value};

use crate::collection::defra_collection_truncate;
use crate::encrypted_index::{defra_encrypted_index_list, defra_encrypted_index_new};
use crate::index::{defra_index_delete, defra_index_list, defra_index_new};
use crate::test_support::{
    add_collection, close, cstr, err, local_options, node, node_with, ok, ok_json, query, take,
    wait_for, Named,
};
use crate::txn::{defra_transaction_commit, defra_transaction_create, defra_transaction_discard};
use crate::types::NodeInitOptions;
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
    new_index(handle, name, false);
}

fn new_index(handle: usize, name: &str, unique: bool) {
    let user = Named::new("User");
    let (name, fields) = (cstr(name), cstr("name"));
    ok(unsafe {
        defra_index_new(
            handle,
            name.as_ptr(),
            fields.as_ptr(),
            unique.into(),
            ptr::null(),
            user.options(),
            0,
        )
    });
}

/// Waits until index `name` of `User` on `node` has finished building, so
/// every document it covers is in it.
fn wait_until_built(node: usize, name: &str) {
    let user = Named::new("User");
    wait_for(Duration::from_secs(10), || {
        let listed = ok_json(unsafe { defra_index_list(node, user.options(), 0) });
        let index = listed
            .as_array()
            .and_then(|indexes| {
                indexes
                    .iter()
                    .find(|index| index["Description"]["Name"] == name)
            })
            .unwrap_or_else(|| panic!("index {name} is listed: {listed}"));
        let status = &index["Execution"]["Status"];
        match status
            .as_u64()
            .map(|status| ActionStatus::new(status as u16))
        {
            Some(ActionStatus::COMPLETED) => Ok(()),
            Some(ActionStatus::ERRORED) => panic!("the build failed: {index}"),
            _ => Err(format!("still building: {index}")),
        }
    })
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

/// Deleting a missing index answers Go's error on a node and on a
/// transaction alike.
#[test]
fn deleting_a_missing_index_answers_alike_on_a_node_and_a_transaction() {
    let node = users();
    let txn = begin(node);
    let (user, name) = (Named::new("User"), cstr("missing"));
    let delete = |handle: usize| {
        err(unsafe { defra_index_delete(handle, name.as_ptr(), user.options(), 0) })
    };

    let expected = "index with name doesn't exists. Name: missing";
    assert_eq!(delete(node), expected, "on the node");
    assert_eq!(delete(txn), expected, "on the transaction");

    defra_transaction_discard(txn);
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

/// A unique index created in a transaction, after a document the transaction
/// added, holds that document once built: a second document with the same
/// value is refused, which only an entry for the first one can cause.
#[test]
fn an_index_created_in_a_transaction_covers_its_own_documents() {
    let node = node();
    add_collection(node, "type User { name: String }");
    let add_b = r#"mutation { add_User(input: {name: "B"}) { _docID } }"#;
    let txn = begin(node);
    let added = query(txn, add_b);
    assert!(added.get("errors").is_none(), "{added}");

    new_index(txn, "by_name", true);
    ok(defra_transaction_commit(txn));
    wait_until_built(node, "by_name");

    let duplicate = query(node, add_b);
    assert!(
        duplicate.get("errors").is_some(),
        "the index holds the first B: {duplicate}"
    );
    assert_eq!(user_names(node), json!([{"name": "B"}]));
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
    assert_eq!(
        view(txn)["data"]["UserView"],
        json!([{"name": "A"}]),
        "the transaction queries it"
    );
    assert!(view(node).get("errors").is_some(), "outside, no view yet");

    ok(defra_transaction_commit(txn));
    assert_eq!(view(node)["data"]["UserView"], json!([{"name": "A"}]));
    close(node);
}

/// A refresh on a transaction handle rebuilds the view from what the
/// transaction commits: a document it added shows only once it commits.
#[test]
fn a_view_refreshed_in_a_transaction_shows_its_documents_once_it_commits() {
    let node = users();
    let (view_query, sdl) = (
        cstr("User { name }"),
        cstr("type UserView @materialized(if: true) { name: String }"),
    );
    ok(unsafe { defra_view_add(node, view_query.as_ptr(), sdl.as_ptr(), ptr::null(), 0) });
    let names = |handle: usize| {
        let mut names: Vec<String> = query(handle, "{ UserView { name } }")["data"]["UserView"]
            .as_array()
            .unwrap_or_else(|| panic!("a view answer"))
            .iter()
            .map(|row| row["name"].as_str().unwrap().to_owned())
            .collect();
        names.sort();
        names
    };

    let txn = begin(node);
    query(
        txn,
        r#"mutation { add_User(input: {name: "B"}) { _docID } }"#,
    );
    ok(unsafe { crate::view::defra_view_refresh(txn, Default::default(), 0) });
    assert_eq!(names(node), ["A"], "outside, the view is as it was");

    ok(defra_transaction_commit(txn));
    assert_eq!(names(node), ["A", "B"], "rebuilt from what committed");
    close(node);
}

#[test]
fn an_encrypted_index_deleted_in_a_discarded_transaction_stays() {
    let node = users();
    let (collection, field) = (cstr("User"), cstr("name"));
    ok(unsafe {
        defra_encrypted_index_new(node, collection.as_ptr(), field.as_ptr(), ptr::null(), 0)
    });
    let count = |handle: usize| {
        let listed = ok_json(unsafe { defra_encrypted_index_list(handle, collection.as_ptr(), 0) });
        listed.as_array().map(Vec::len).unwrap_or_default()
    };

    let txn = begin(node);
    ok(unsafe {
        crate::encrypted_index::defra_encrypted_index_delete(
            txn,
            collection.as_ptr(),
            field.as_ptr(),
            0,
        )
    });
    assert_eq!(count(txn), 0, "deleted in the transaction");
    assert_eq!(count(node), 1, "still there outside");

    defra_transaction_discard(txn);
    assert_eq!(count(node), 1);
    close(node);
}

/// Go refuses a filtered truncate in a transaction
/// (`ErrFilteredTruncateInTransaction`): it cannot be run apart from it.
#[test]
fn a_filtered_truncate_on_a_transaction_is_refused() {
    let node = users();
    let txn = begin(node);
    let (user, filter) = (Named::new("User"), cstr(r#"{"name": {"_eq": "A"}}"#));

    let refused = err(unsafe {
        crate::collection::defra_collection_truncate_with_filter(
            txn,
            user.options(),
            0,
            filter.as_ptr(),
        )
    });
    assert!(
        refused.contains("filtered truncate cannot run in a transaction"),
        "{refused}"
    );
    defra_transaction_discard(txn);
    assert_eq!(user_names(node), json!([{"name": "A"}]), "nothing removed");
    close(node);
}

/// A truncate on a transaction handle that already added a document: the
/// truncate removes what was committed at once, and the transaction's own
/// document, written apart from the truncate, lands when it commits.
#[test]
fn a_truncate_after_a_write_in_the_same_transaction_keeps_that_write() {
    let node = users();
    let txn = begin(node);
    query(
        txn,
        r#"mutation { add_User(input: {name: "B"}) { _docID } }"#,
    );
    let user = Named::new("User");

    ok(unsafe { defra_collection_truncate(txn, user.options(), 0) });
    assert_eq!(user_names(node), json!([]), "the committed A is gone");

    let committed = take(defra_transaction_commit(txn));
    assert!(committed.1.is_ok(), "the commit: {committed:?}");
    assert_eq!(user_names(node), json!([{"name": "B"}]), "B landed");
    close(node);
}

/// A node holding `Note { embedding }`, a vector of three numbers.
fn notes() -> usize {
    let node = node();
    add_collection(node, "type Note { embedding: [Float32!] }");
    node
}

fn new_vector_index(handle: usize, name: &str) -> Value {
    let (collection, name, fields) = (Named::new("Note"), cstr(name), cstr("embedding"));
    let vector = cstr(r#"{"Algorithm": "HNSW", "Dimensions": 3}"#);
    ok_json(unsafe {
        defra_index_new(
            handle,
            name.as_ptr(),
            fields.as_ptr(),
            0,
            vector.as_ptr(),
            collection.options(),
            0,
        )
    })
}

fn note_index_names(handle: usize) -> Vec<String> {
    let collection = Named::new("Note");
    let listed = ok_json(unsafe { defra_index_list(handle, collection.options(), 0) });
    listed
        .as_array()
        .unwrap_or_else(|| panic!("an index list: {listed}"))
        .iter()
        .map(|index| index["Description"]["Name"].as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn a_vector_index_is_created_on_a_node() {
    let node = notes();
    let index = new_vector_index(node, "by_embedding");
    assert_eq!(index["KindDescription"]["Algorithm"], "HNSW", "{index}");
    assert_eq!(index["KindDescription"]["Dimensions"], 3, "{index}");
    assert_eq!(note_index_names(node), ["by_embedding"]);
    close(node);
}

#[test]
fn a_vector_index_created_in_a_transaction_is_seen_only_in_it_until_it_commits() {
    let node = notes();
    let txn = begin(node);
    let index = new_vector_index(txn, "by_embedding");
    assert_eq!(index["KindDescription"]["Algorithm"], "HNSW", "{index}");
    assert_eq!(note_index_names(txn), ["by_embedding"]);
    assert!(note_index_names(node).is_empty(), "outside, nothing yet");

    ok(defra_transaction_commit(txn));
    assert_eq!(note_index_names(node), ["by_embedding"]);
    close(node);
}

/// A node with node access control on, owned by `owner`, holding `User`.
fn owned_users(owner: usize) -> usize {
    let node = node_with(NodeInitOptions {
        identity_ptr: owner,
        enable_node_acp: 1,
        ..local_options()
    });
    let sdl = cstr("type User { name: String }");
    ok(unsafe { crate::collection::defra_collection_add(node, sdl.as_ptr(), owner) });
    node
}

fn new_identity() -> usize {
    let result = unsafe { crate::identities::defra_identity_new(ptr::null()) };
    assert_eq!(result.status, 0);
    result.identity_ptr
}

/// Node access control decides a call on a transaction handle as on the node:
/// a caller without the permission is refused, the owner is not.
#[test]
fn node_access_control_refuses_an_index_create_on_a_transaction_handle() {
    let (owner, stranger) = (new_identity(), new_identity());
    let node = owned_users(owner);
    let txn = begin(node);
    let (user, name, fields) = (Named::new("User"), cstr("by_name"), cstr("name"));
    let create = |identity: usize| unsafe {
        defra_index_new(
            txn,
            name.as_ptr(),
            fields.as_ptr(),
            0,
            ptr::null(),
            user.options(),
            identity,
        )
    };

    let refused = err(create(stranger));
    assert!(refused.contains("not authorized"), "{refused}");
    ok(create(owner));

    defra_transaction_discard(txn);
    close(node);
    crate::identities::defra_identity_free(owner);
    crate::identities::defra_identity_free(stranger);
}
