//! Save-many, purge and the replication filter, called as a C caller calls
//! them.

use std::ffi::{c_char, c_int, CStr};
use std::ptr::null;
use std::sync::atomic::{AtomicUsize, Ordering};

use serde_json::{json, Value};

use crate::document::{defra_document_purge, defra_document_save_many};
use crate::state::NODES;
use crate::test_support::{
    add_collection, close, cstr, err, local_options, node, node_with, ok, ok_json, query, Named,
};
use crate::txn::{defra_transaction_commit, defra_transaction_create};
use crate::types::NodeInitOptions;

const USERS: &str = "type User { name: String @index age: Int }";

fn users_node() -> usize {
    let node = node();
    add_collection(node, USERS);
    node
}

fn add(node: usize, name: &str, age: i64) -> String {
    let data = query(
        node,
        &format!(r#"mutation {{ add_User(input: {{name: "{name}", age: {age}}}) {{ _docID }} }}"#),
    );
    data["data"]["add_User"][0]["_docID"]
        .as_str()
        .unwrap()
        .to_owned()
}

fn users(node: usize) -> Value {
    query(
        node,
        "query { User(order: {name: ASC}) { _docID name age } }",
    )["data"]["User"]
        .clone()
}

fn begin(node: usize) -> usize {
    let txn = defra_transaction_create(node, 0);
    assert_eq!(txn.status, 0);
    txn.txn_ptr
}

fn purge(handle: usize, ids: &Value) -> Result<Value, String> {
    let user = Named::new("User");
    let ids = cstr(&ids.to_string());
    let result = unsafe { defra_document_purge(handle, ids.as_ptr(), user.options(), 0) };
    match crate::test_support::take(result) {
        (_, Ok(value)) => Ok(serde_json::from_str(&value).unwrap()),
        (_, Err(error)) => Err(error),
    }
}

fn save_many(handle: usize, docs: &Value) -> Value {
    let user = Named::new("User");
    let docs = cstr(&docs.to_string());
    ok_json(unsafe {
        defra_document_save_many(handle, docs.as_ptr(), 0, null(), user.options(), 0)
    })
}

#[test]
fn purging_documents_in_a_transaction_removes_them_on_commit() {
    let node = users_node();
    let alice = add(node, "Alice", 40);
    let bob = add(node, "Bob", 7);

    let txn = begin(node);
    assert_eq!(purge(txn, &json!([alice])).unwrap(), json!({ "Count": 1 }));
    assert_eq!(
        users(node).as_array().unwrap().len(),
        2,
        "not yet committed"
    );
    ok(defra_transaction_commit(txn));
    assert_eq!(
        users(node),
        json!([{ "_docID": bob, "name": "Bob", "age": 7 }])
    );

    assert_eq!(
        purge(node, &json!([bob])).unwrap(),
        json!({ "Count": 1 }),
        "without a transaction"
    );
    assert_eq!(users(node), json!([]));

    let error = purge(node, &json!(["not-a-docid"])).unwrap_err();
    assert!(!error.is_empty());
    let user = Named::new("User");
    let not_json = cstr("not json");
    let error = err(unsafe { defra_document_purge(node, not_json.as_ptr(), user.options(), 0) });
    assert!(error.contains("invalid document IDs"), "{error}");
    close(node);
}

#[test]
fn save_many_adds_and_updates_in_order() {
    let node = users_node();
    let ids = save_many(
        node,
        &json!([{ "name": "Alice", "age": 40 }, { "name": "Bob", "age": 7 }]),
    );
    let ids = ids.as_array().unwrap().clone();
    assert_eq!(ids.len(), 2);

    let txn = begin(node);
    let saved = save_many(
        txn,
        &json!([
            { "_docID": ids[0], "name": "Alice", "age": 41 },
            { "name": "Carol", "age": 3 },
        ]),
    );
    assert_eq!(saved[0], ids[0]);
    let carol = saved[1].clone();
    assert_eq!(
        users(node).as_array().unwrap().len(),
        2,
        "not yet committed"
    );
    ok(defra_transaction_commit(txn));
    assert_eq!(
        users(node),
        json!([
            { "_docID": ids[0], "name": "Alice", "age": 41 },
            { "_docID": ids[1], "name": "Bob", "age": 7 },
            { "_docID": carol, "name": "Carol", "age": 3 },
        ])
    );

    let user = Named::new("User");
    let not_array = cstr(r#"{"name": "Dan"}"#);
    let error = err(unsafe {
        defra_document_save_many(node, not_array.as_ptr(), 0, null(), user.options(), 0)
    });
    assert!(error.contains("invalid documents JSON array"), "{error}");
    close(node);
}

static FILTER_CALLS: AtomicUsize = AtomicUsize::new(0);
static FILTER_MISMATCHES: AtomicUsize = AtomicUsize::new(0);

/// Refuses every document, counting calls whose arguments are the expected
/// ones apart from the rest: a panic here would abort the test binary.
unsafe extern "C" fn refuse_with_context(
    context: usize,
    collection_id: *const c_char,
    collection_name: *const c_char,
    doc_id: *const c_char,
    fields_json: *const c_char,
) -> c_int {
    let read = |ptr| {
        unsafe { CStr::from_ptr(ptr) }
            .to_string_lossy()
            .into_owned()
    };
    let fields: Option<Value> = serde_json::from_str(&read(fields_json)).ok();
    let expected = context == 7
        && read(collection_id) == "collection"
        && read(collection_name) == "Log"
        && read(doc_id) == "bae-doc"
        && fields
            == Some(
                json!({ "name": "Alice", "blockNumber": 42, "big": u64::MAX, "weight": 3.0, "raw": "AQI=" }),
            );
    match expected {
        true => FILTER_CALLS.fetch_add(1, Ordering::SeqCst),
        false => FILTER_MISMATCHES.fetch_add(1, Ordering::SeqCst),
    };
    0
}

#[test]
fn the_replication_filter_option_asks_the_callback() {
    let filtered = node_with(NodeInitOptions {
        replication_filter: Some(refuse_with_context),
        replication_filter_context: 7,
        ..local_options()
    });
    let filter = NODES
        .get(filtered, |state| {
            state.database.options().replication_filter.clone()
        })
        .unwrap()
        .expect("the node holds the filter");
    let mut fields = db::replication_filter::FieldValues::default();
    fields.insert("name".to_owned(), ciborium::Value::Text("Alice".to_owned()));
    fields.insert(
        "blockNumber".to_owned(),
        ciborium::Value::Integer(42.into()),
    );
    fields.insert("big".to_owned(), ciborium::Value::Integer(u64::MAX.into()));
    fields.insert("weight".to_owned(), ciborium::Value::Float(3.0));
    fields.insert("raw".to_owned(), ciborium::Value::Bytes(vec![1, 2]));
    let kept = filter.allow_replication("collection", "Log", "bae-doc", &fields);
    assert!(!kept, "the callback refused it");
    assert_eq!(FILTER_CALLS.load(Ordering::SeqCst), 1);
    assert_eq!(
        FILTER_MISMATCHES.load(Ordering::SeqCst),
        0,
        "each value keeps its kind"
    );

    let kept = filter.allow_replication("collection", "Log", "bae-\0doc", &fields);
    assert!(!kept, "a DocID with a NUL is refused");
    assert_eq!(
        FILTER_CALLS.load(Ordering::SeqCst) + FILTER_MISMATCHES.load(Ordering::SeqCst),
        1,
        "without reaching the callback"
    );

    fields.insert("ratio".to_owned(), ciborium::Value::Float(f64::NAN));
    filter.allow_replication("collection", "Log", "bae-doc", &fields);
    assert_eq!(
        FILTER_CALLS.load(Ordering::SeqCst),
        2,
        "the callback still decides, without the field JSON cannot carry"
    );
    assert_eq!(FILTER_MISMATCHES.load(Ordering::SeqCst), 0);
    close(filtered);

    let without = node();
    let filter = NODES
        .get(without, |state| {
            state.database.options().replication_filter.clone()
        })
        .unwrap();
    assert!(filter.is_none(), "no callback, no filter");
    close(without);
}
