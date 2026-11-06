use std::ptr;

use crate::identities::{defra_identity_free, defra_identity_new};
use crate::state::NODES;
use crate::test_support::{
    add_collection, close, cstr, err, local_options, node, node_with, ok_json, query,
};
use crate::txn::{defra_transaction_commit, defra_transaction_create, defra_transaction_discard};
use crate::types::NodeInitOptions;

use super::{dac_bypass, defra_query_execute};

#[test]
fn node_identity_is_not_promoted_to_nac_dac_bypass() {
    let node = node_with(NodeInitOptions {
        enable_signing: 1,
        ..local_options()
    });
    let node_did = NODES
        .get(node, |state| state.identity_did())
        .flatten()
        .expect("a signing node has an identity");

    let bypass = crate::call::wait(async { Ok(dac_bypass(node, Some(&node_did)).await) })
        .expect("the check runs");

    assert!(!bypass);
    close(node);
}

#[test]
fn a_query_answers_with_its_graphql_result() {
    let node = node();
    add_collection(node, "type User { name: String }");

    let response = query(node, "{ User { name } }");
    assert_eq!(response["data"]["User"], serde_json::json!([]));
    close(node);
}

#[test]
fn a_mutation_is_visible_to_a_later_query() {
    let node = node();
    add_collection(node, "type User { name: String }");

    let response = query(
        node,
        r#"mutation { add_User(input: {name: "Alice"}) { _docID name } }"#,
    );
    assert_eq!(response["data"]["add_User"][0]["name"], "Alice");

    let response = query(node, "{ User { name } }");
    assert_eq!(response["data"]["User"][0]["name"], "Alice");
    close(node);
}

#[test]
fn a_query_with_variables_and_an_operation_name_runs_that_operation() {
    let node = node();
    add_collection(node, "type User { name: String }");
    query(
        node,
        r#"mutation { add_User(input: {name: "Alice"}) { _docID } }"#,
    );

    let request = cstr(
        "query Named($name: String) { User(filter: {name: {_eq: $name}}) { name } } \
         query Other { User { _docID } }",
    );
    let operation = cstr("Named");
    let variables = cstr(r#"{"name": "Alice"}"#);
    let response = ok_json(unsafe {
        defra_query_execute(
            node,
            request.as_ptr(),
            0,
            operation.as_ptr(),
            variables.as_ptr(),
        )
    });
    assert_eq!(response["data"]["User"][0]["name"], "Alice");
    close(node);
}

#[test]
fn an_invalid_handle_fails() {
    let request = cstr("{ User { name } }");
    let error =
        err(unsafe { defra_query_execute(0, request.as_ptr(), 0, ptr::null(), ptr::null()) });
    assert!(
        error.contains("invalid"),
        "should indicate invalid handle: {error}"
    );
}

#[test]
fn invalid_variables_json_fails() {
    let node = node();
    add_collection(node, "type User { name: String }");

    let request = cstr("{ User { name } }");
    let variables = cstr("not valid json");
    let error = err(unsafe {
        defra_query_execute(node, request.as_ptr(), 0, ptr::null(), variables.as_ptr())
    });
    assert!(
        error.contains("variables"),
        "should indicate the variables: {error}"
    );
    close(node);
}

#[test]
fn a_transaction_handle_runs_the_query_in_the_transaction() {
    let node = node();
    add_collection(node, "type User { name: String }");

    let txn = defra_transaction_create(node, 0);
    assert_eq!(txn.status, 0);
    query(
        txn.txn_ptr,
        r#"mutation { add_User(input: {name: "Bob"}) { _docID } }"#,
    );

    // Uncommitted: the transaction sees the document, the node does not.
    assert_eq!(
        query(txn.txn_ptr, "{ User { name } }")["data"]["User"][0]["name"],
        "Bob"
    );
    assert_eq!(
        query(node, "{ User { name } }")["data"]["User"],
        serde_json::json!([])
    );

    crate::test_support::ok(defra_transaction_commit(txn.txn_ptr));
    assert_eq!(
        query(node, "{ User { name } }")["data"]["User"][0]["name"],
        "Bob"
    );
    close(node);
}

#[test]
fn a_discarded_transaction_leaves_nothing() {
    let node = node();
    add_collection(node, "type User { name: String }");

    let txn = defra_transaction_create(node, 0);
    assert_eq!(txn.status, 0);
    query(
        txn.txn_ptr,
        r#"mutation { add_User(input: {name: "Bob"}) { _docID } }"#,
    );
    defra_transaction_discard(txn.txn_ptr);

    assert_eq!(
        query(node, "{ User { name } }")["data"]["User"],
        serde_json::json!([])
    );
    close(node);
}

#[test]
fn an_identity_handle_acts_as_that_identity() {
    let node = node();
    let identity = unsafe { defra_identity_new(ptr::null()) };
    assert_eq!(identity.status, 0);

    let response = crate::test_support::query_as(node, "{ __typename }", identity.identity_ptr);
    assert!(response.get("data").is_some(), "{response}");

    defra_identity_free(identity.identity_ptr);
    close(node);
}
