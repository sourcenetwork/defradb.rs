//! Save-many, purge, the replication filter and the libp2p node options,
//! called as a C caller calls them.

use std::ffi::{c_char, c_int, CStr, CString};
use std::ptr::null;
use std::sync::atomic::{AtomicUsize, Ordering};

use serde_json::{json, Value};

use crate::collection::{purge_documents, save_many_documents};
use crate::node::{new_node, node_close};
use crate::query::exec_request;
use crate::schema::add_schema;
use crate::state::NODES;
use crate::txn::{begin_txn, commit_txn};
use crate::types::{defra_free_string, FfiResult, NodeInitOptions};

const USERS: &str = "type User { name: String @index age: Int }";

fn cstr(text: &str) -> CString {
    CString::new(text).unwrap()
}

/// The call's JSON value, or its error; both strings freed.
fn take(result: FfiResult) -> Result<Value, String> {
    let read = |ptr: *mut c_char| {
        (!ptr.is_null()).then(|| {
            let text = unsafe { CStr::from_ptr(ptr) }
                .to_string_lossy()
                .into_owned();
            unsafe { defra_free_string(ptr) };
            text
        })
    };
    let value = read(result.value);
    let error = read(result.error);
    match result.status {
        0 => Ok(value.map_or(Value::Null, |text| {
            serde_json::from_str(&text).unwrap_or(Value::String(text))
        })),
        _ => Err(error.unwrap_or_default()),
    }
}

fn node_with(options: NodeInitOptions) -> usize {
    assert!(crate::runtime::init_runtime());
    let result = new_node(options);
    assert_eq!(result.status, 0);
    let node = result.node_ptr;
    let sdl = cstr(USERS);
    take(unsafe { add_schema(node, null(), sdl.as_ptr()) }).unwrap();
    node
}

fn node() -> usize {
    node_with(NodeInitOptions::default())
}

fn gql(node: usize, txn: Option<&CStr>, request: &str) -> Value {
    let request = cstr(request);
    let txn = txn.map_or(null(), |txn| txn.as_ptr());
    let response =
        take(unsafe { exec_request(node, txn, null(), request.as_ptr(), null(), null(), null()) })
            .unwrap();
    response["data"].clone()
}

fn add(node: usize, name: &str, age: i64) -> String {
    let data = gql(
        node,
        None,
        &format!(r#"mutation {{ add_User(input: {{name: "{name}", age: {age}}}) {{ _docID }} }}"#),
    );
    data["add_User"][0]["_docID"].as_str().unwrap().to_owned()
}

fn users(node: usize) -> Value {
    gql(
        node,
        None,
        "query { User(order: {name: ASC}) { _docID name age } }",
    )["User"]
        .clone()
}

fn begin(node: usize) -> CString {
    let txn = begin_txn(node, 0);
    assert_eq!(txn.status, 0);
    let id = unsafe { CStr::from_ptr(txn.txn_id) }.to_owned();
    unsafe { defra_free_string(txn.txn_id) };
    id
}

#[test]
fn purge_documents_removes_them_in_a_transaction_on_commit() {
    let node = node();
    let alice = add(node, "Alice", 40);
    let bob = add(node, "Bob", 7);
    let txn = begin(node);

    let (collection, ids) = (cstr("User"), cstr(&json!([alice]).to_string()));
    let purged = take(unsafe {
        purge_documents(
            node,
            txn.as_ptr(),
            null(),
            collection.as_ptr(),
            ids.as_ptr(),
        )
    })
    .unwrap();
    assert_eq!(purged, json!({ "purged": 1 }));
    assert_eq!(
        users(node).as_array().unwrap().len(),
        2,
        "not yet committed"
    );

    take(unsafe { commit_txn(node, txn.as_ptr()) }).unwrap();
    assert_eq!(
        users(node),
        json!([{ "_docID": bob, "name": "Bob", "age": 7 }])
    );

    let ids = cstr(&json!([bob]).to_string());
    let purged =
        take(unsafe { purge_documents(node, null(), null(), collection.as_ptr(), ids.as_ptr()) })
            .unwrap();
    assert_eq!(purged, json!({ "purged": 1 }), "without a transaction");
    assert_eq!(users(node), json!([]));

    let bad = cstr(r#"["not-a-docid"]"#);
    let error =
        take(unsafe { purge_documents(node, null(), null(), collection.as_ptr(), bad.as_ptr()) })
            .unwrap_err();
    assert!(error.contains("invalid DocID"), "{error}");
    node_close(node);
}

#[test]
fn save_many_documents_adds_and_updates_in_order() {
    let node = node();
    let collection = cstr("User");
    let docs = cstr(r#"[{"name": "Alice", "age": 40}, {"name": "Bob", "age": 7}]"#);
    let saved = take(unsafe {
        save_many_documents(
            node,
            null(),
            null(),
            collection.as_ptr(),
            docs.as_ptr(),
            null(),
        )
    })
    .unwrap();
    let ids = saved["docIDs"].as_array().unwrap().clone();
    assert_eq!(ids.len(), 2);

    let txn = begin(node);
    let update = cstr(
        &json!([
            { "_docID": ids[0], "name": "Alice", "age": 41 },
            { "name": "Carol", "age": 3 },
        ])
        .to_string(),
    );
    let saved = take(unsafe {
        save_many_documents(
            node,
            txn.as_ptr(),
            null(),
            collection.as_ptr(),
            update.as_ptr(),
            null(),
        )
    })
    .unwrap();
    let carol = saved["docIDs"][1].clone();
    assert_eq!(saved["docIDs"][0], ids[0]);
    assert_eq!(
        users(node).as_array().unwrap().len(),
        2,
        "not yet committed"
    );

    take(unsafe { commit_txn(node, txn.as_ptr()) }).unwrap();
    assert_eq!(
        users(node),
        json!([
            { "_docID": ids[0], "name": "Alice", "age": 41 },
            { "_docID": ids[1], "name": "Bob", "age": 7 },
            { "_docID": carol, "name": "Carol", "age": 3 },
        ])
    );

    let bad = cstr(r#"{"encryptDoc": "yes"}"#);
    let error = take(unsafe {
        save_many_documents(
            node,
            null(),
            null(),
            collection.as_ptr(),
            docs.as_ptr(),
            bad.as_ptr(),
        )
    })
    .unwrap_err();
    assert!(error.contains("invalid options_json"), "{error}");
    node_close(node);
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
        ..NodeInitOptions::default()
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
    node_close(filtered);

    let without = node();
    let filter = NODES
        .get(without, |state| {
            state.database.options().replication_filter.clone()
        })
        .unwrap();
    assert!(filter.is_none(), "no callback, no filter");
    node_close(without);
}
