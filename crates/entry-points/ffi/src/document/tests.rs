use std::ptr;

use serde_json::{json, Value};

use super::*;
use crate::test_support::{add_collection, close, cstr, err, node, ok, ok_json, query, Named};

fn users() -> (usize, Named) {
    let node = node();
    add_collection(node, "type User { name: String age: Int }");
    (node, Named::new("User"))
}

fn add(node: usize, users: &Named, json: &str) -> Vec<String> {
    let json = cstr(json);
    let ids = ok_json(unsafe {
        defra_document_add(node, json.as_ptr(), 0, ptr::null(), users.options(), 0)
    });
    serde_json::from_value(ids).expect("add answers with document IDs")
}

#[test]
fn add_answers_with_document_ids_for_one_or_many() {
    let (node, users) = users();
    assert_eq!(add(node, &users, r#"{"name": "A", "age": 1}"#).len(), 1);
    assert_eq!(
        add(node, &users, r#"[{"name": "B"}, {"name": "C"}]"#).len(),
        2
    );
    assert_eq!(
        query(node, "{ User { name } }")["data"]["User"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    close(node);
}

#[test]
fn get_answers_with_the_documents_fields_and_id() {
    let (node, users) = users();
    let id = add(node, &users, r#"{"name": "A", "age": 1}"#).remove(0);

    let doc_id = cstr(&id);
    let doc = ok_json(unsafe { defra_document_get(node, doc_id.as_ptr(), 0, users.options(), 0) });
    assert_eq!(doc["_docID"], Value::from(id.as_str()));
    assert_eq!(doc["name"], "A");
    assert_eq!(doc["age"], 1);
    close(node);
}

#[test]
fn update_by_id_and_by_filter() {
    let (node, users) = users();
    let id = add(node, &users, r#"{"name": "A", "age": 1}"#).remove(0);
    add(node, &users, r#"{"name": "B", "age": 1}"#);

    let doc_id = cstr(&id);
    let updater = cstr(r#"{"age": 2}"#);
    assert_eq!(
        ok(unsafe {
            defra_document_update(
                node,
                doc_id.as_ptr(),
                ptr::null(),
                updater.as_ptr(),
                users.options(),
                0,
            )
        }),
        ""
    );

    let filter = cstr(r#"{"age": {"_eq": 1}}"#);
    let updater = cstr(r#"{"age": 5}"#);
    let result = ok_json(unsafe {
        defra_document_update(
            node,
            ptr::null(),
            filter.as_ptr(),
            updater.as_ptr(),
            users.options(),
            0,
        )
    });
    assert_eq!(result["Count"], 1);

    let ages = query(node, "{ User(order: {name: ASC}) { age } }");
    assert_eq!(ages["data"]["User"], json!([{"age": 2}, {"age": 5}]));
    close(node);
}

#[test]
fn delete_by_id_and_by_filter() {
    let (node, users) = users();
    let id = add(node, &users, r#"{"name": "A"}"#).remove(0);
    add(node, &users, r#"[{"name": "B"}, {"name": "C"}]"#);

    let doc_id = cstr(&id);
    ok(unsafe { defra_document_delete(node, doc_id.as_ptr(), ptr::null(), users.options(), 0) });
    // Deleting it again finds nothing.
    err(unsafe { defra_document_delete(node, doc_id.as_ptr(), ptr::null(), users.options(), 0) });

    let filter = cstr(r#"{"name": {"_eq": "B"}}"#);
    let result = ok_json(unsafe {
        defra_document_delete(node, ptr::null(), filter.as_ptr(), users.options(), 0)
    });
    assert_eq!(result["Count"], 1);
    assert_eq!(
        query(node, "{ User { name } }")["data"]["User"],
        json!([{"name": "C"}])
    );

    // The deleted document is still there to show.
    let doc = ok_json(unsafe { defra_document_get(node, doc_id.as_ptr(), 1, users.options(), 0) });
    assert_eq!(doc["name"], "A");
    close(node);
}

#[test]
fn update_or_delete_without_an_id_or_filter_fails() {
    let (node, users) = users();
    let updater = cstr(r#"{"age": 2}"#);
    let error = err(unsafe {
        defra_document_update(
            node,
            ptr::null(),
            ptr::null(),
            updater.as_ptr(),
            users.options(),
            0,
        )
    });
    assert_eq!(error, "operation requires a DocID or filter");
    err(unsafe { defra_document_delete(node, ptr::null(), ptr::null(), users.options(), 0) });
    close(node);
}

#[test]
fn a_document_id_that_is_not_one_fails_before_running() {
    let (node, users) = users();
    let injected = cstr(r#"x") { _docID } } mutation { delete_User(filter: {}"#);
    err(unsafe { defra_document_delete(node, injected.as_ptr(), ptr::null(), users.options(), 0) });
    close(node);
}

#[test]
fn an_unknown_collection_fails() {
    let node = node();
    let missing = Named::new("Missing");
    let json = cstr(r#"{"name": "A"}"#);
    let error = err(unsafe {
        defra_document_add(node, json.as_ptr(), 0, ptr::null(), missing.options(), 0)
    });
    assert_eq!(error, "collection not found");
    close(node);
}
