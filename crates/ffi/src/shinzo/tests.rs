//! The Shinzo FFI functions, called as a C caller calls them.

use std::ffi::{c_char, c_int, CStr, CString};
use std::ptr::null;
use std::sync::atomic::{AtomicUsize, Ordering};

use serde_json::{json, Value};

use super::*;
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

fn gql(node: usize, txn: Option<&CString>, request: &str) -> Value {
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

#[test]
fn purge_documents_removes_them_in_a_transaction_on_commit() {
    let node = node();
    let alice = add(node, "Alice", 40);
    let txn = begin_txn(node, 0);
    assert_eq!(txn.status, 0);
    let txn_id = unsafe { CStr::from_ptr(txn.txn_id) }.to_owned();
    unsafe { defra_free_string(txn.txn_id) };

    let (collection, ids) = (cstr("User"), cstr(&json!([alice]).to_string()));
    let purged = take(unsafe {
        purge_documents(
            node,
            txn_id.as_ptr(),
            null(),
            collection.as_ptr(),
            ids.as_ptr(),
            true,
        )
    })
    .unwrap();
    assert_eq!(purged, json!({ "purged": 1 }));
    assert_eq!(
        users(node).as_array().unwrap().len(),
        1,
        "not yet committed"
    );

    take(unsafe { commit_txn(node, txn_id.as_ptr()) }).unwrap();
    assert_eq!(users(node), json!([]));

    let bad = cstr(r#"["not-a-docid"]"#);
    let error = take(unsafe {
        purge_documents(
            node,
            null(),
            null(),
            collection.as_ptr(),
            bad.as_ptr(),
            false,
        )
    })
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
            null(),
        )
    })
    .unwrap();
    let ids = saved["docIDs"].as_array().unwrap().clone();
    assert_eq!(ids.len(), 2);

    let update = cstr(&json!([{ "_docID": ids[0], "name": "Alice", "age": 41 }]).to_string());
    let saved = take(unsafe {
        save_many_documents(
            node,
            null(),
            null(),
            collection.as_ptr(),
            update.as_ptr(),
            null(),
            null(),
        )
    })
    .unwrap();
    assert_eq!(saved["docIDs"], json!([ids[0]]));
    assert_eq!(
        users(node),
        json!([
            { "_docID": ids[0], "name": "Alice", "age": 41 },
            { "_docID": ids[1], "name": "Bob", "age": 7 },
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
            null(),
        )
    })
    .unwrap_err();
    assert!(error.contains("invalid options_json"), "{error}");
    node_close(node);
}

#[test]
fn kv_export_and_import_move_documents_between_nodes() {
    let (source, target) = (node(), node());
    let alice = add(source, "Alice", 40);
    let bob = add(source, "Bob", 7);
    let dir = tempfile::tempdir().unwrap();
    let path = cstr(dir.path().join("users.kv").to_str().unwrap());
    let collection = cstr("User");

    let exported = take(unsafe {
        kv_export(
            source,
            null(),
            collection.as_ptr(),
            null(),
            null(),
            path.as_ptr(),
        )
    })
    .unwrap();
    assert!(exported["frames"].as_u64().unwrap() > 0);
    assert_eq!(exported["mapping"]["collectionName"], "User");
    let mapping = take(unsafe { kv_field_mapping(source, null(), collection.as_ptr()) }).unwrap();
    assert_eq!(mapping["fields"], exported["mapping"]["fields"]);
    assert_eq!(
        mapping["collectionShortID"],
        exported["mapping"]["collectionShortID"]
    );

    let mapping = cstr(&exported["mapping"].to_string());
    let imported =
        take(unsafe { kv_import(target, null(), path.as_ptr(), mapping.as_ptr()) }).unwrap();
    assert_eq!(imported["frames"], exported["frames"]);
    assert_eq!(imported["documents"], 2);
    assert_eq!(
        users(target),
        json!([
            { "_docID": alice, "name": "Alice", "age": 40 },
            { "_docID": bob, "name": "Bob", "age": 7 },
        ])
    );

    let missing = cstr(dir.path().join("missing.kv").to_str().unwrap());
    let error = take(unsafe { kv_import(target, null(), missing.as_ptr(), null()) }).unwrap_err();
    assert!(error.contains("failed to read"), "{error}");
    node_close(source);
    node_close(target);
}

static FILTER_CALLS: AtomicUsize = AtomicUsize::new(0);

unsafe extern "C" fn refuse_with_context(
    context: usize,
    collection_id: *const c_char,
    doc_id: *const c_char,
    fields_json: *const c_char,
) -> c_int {
    let read = |ptr| {
        unsafe { CStr::from_ptr(ptr) }
            .to_string_lossy()
            .into_owned()
    };
    assert_eq!(context, 7);
    assert_eq!(read(collection_id), "collection");
    assert_eq!(read(doc_id), "bae-doc");
    assert_eq!(read(fields_json), "{}");
    FILTER_CALLS.fetch_add(1, Ordering::SeqCst);
    0
}

#[test]
fn node_options_set_the_retention_rule_and_the_replication_filter() {
    let retention = cstr(r#"{"col-log": {"field": "blockNumber", "floor": 100}}"#);
    let node = node_with(NodeInitOptions {
        retention_json: retention.as_ptr(),
        replication_filter: Some(refuse_with_context),
        replication_filter_context: 7,
        ..NodeInitOptions::default()
    });
    let (retention, filter) = NODES
        .get(node, |state| {
            let options = state.database.options();
            (
                options.retention_rule.clone(),
                options.replication_filter.clone(),
            )
        })
        .unwrap();
    assert!(retention.is_some());
    let kept = filter.unwrap().allow_replication(
        "collection",
        "bae-doc",
        &db::replication_filter::FieldValues::default(),
    );
    assert!(!kept, "the callback refused it");
    assert_eq!(FILTER_CALLS.load(Ordering::SeqCst), 1);
    node_close(node);

    let bad = cstr(r#"{"col-log": {"field": "blockNumber"}}"#);
    let result = new_node(NodeInitOptions {
        retention_json: bad.as_ptr(),
        ..NodeInitOptions::default()
    });
    assert_eq!(result.status, 1);
    let error = unsafe { CStr::from_ptr(result.error) }
        .to_string_lossy()
        .into_owned();
    unsafe { defra_free_string(result.error) };
    assert!(error.contains("invalid retention_json"), "{error}");
}

#[test]
fn batch_options_choose_the_scope_and_the_merkle_scheme() {
    let node = node_with(NodeInitOptions {
        enable_signing: 1,
        ..NodeInitOptions::default()
    });
    let session = cstr("shinzo-batch");
    let (full, scheme) = (cstr("Full"), cstr("DomainSeparated"));
    take(unsafe {
        crate::batch::batch_start_with_options(node, null(), session.as_ptr(), full.as_ptr())
    })
    .unwrap();
    let request = cstr(r#"mutation { add_User(input: {name: "Alice", age: 40}) { _docID } }"#);
    take(unsafe {
        exec_request(
            node,
            null(),
            null(),
            request.as_ptr(),
            null(),
            null(),
            session.as_ptr(),
        )
    })
    .unwrap();
    let signature = take(unsafe {
        crate::batch::batch_sign_with_options(node, null(), session.as_ptr(), scheme.as_ptr())
    })
    .unwrap();
    assert_eq!(signature["merkle_scheme"], "DomainSeparated");
    assert!(
        signature["cid_count"].as_u64().unwrap() > 1,
        "the full scope collects field blocks too: {signature}"
    );

    let nope = cstr("Nope");
    let error = take(unsafe {
        crate::batch::batch_start_with_options(node, null(), session.as_ptr(), nope.as_ptr())
    })
    .unwrap_err();
    assert_eq!(error, "invalid batch_scope: Nope");
    node_close(node);
}

#[cfg(feature = "libp2p")]
#[test]
fn node_options_turn_on_go_shinzos_wire() {
    let libp2p = || {
        defra_node::NodeBuilder::default().with_config(defra_node::NodeConfig {
            transport: defra_node::TransportConfig::Libp2p(Default::default()),
            ..Default::default()
        })
    };
    let wire = |builder: &defra_node::NodeBuilder| match &builder.config().transport {
        defra_node::TransportConfig::Libp2p(config) => config.shinzo_wire,
        _ => unreachable!("a libp2p node"),
    };
    let on = unsafe {
        with_node_options(
            libp2p(),
            &NodeInitOptions {
                shinzo_wire: 1,
                ..NodeInitOptions::default()
            },
        )
    }
    .unwrap();
    assert!(wire(&on));
    let off = unsafe { with_node_options(libp2p(), &NodeInitOptions::default()) }.unwrap();
    assert!(!wire(&off));
}

unsafe extern "C" fn floor_for_logs(
    context: usize,
    collection_id: *const c_char,
    field_out: *mut c_char,
    field_capacity: usize,
    floor_out: *mut i64,
) -> c_int {
    assert_eq!(context, 9);
    if unsafe { CStr::from_ptr(collection_id) }.to_bytes() != b"col-log" {
        return 0;
    }
    let field = b"blockNumber\0";
    assert!(field.len() <= field_capacity);
    unsafe {
        std::ptr::copy_nonoverlapping(field.as_ptr().cast(), field_out, field.len());
        *floor_out = 100;
    }
    1
}

#[test]
fn node_options_take_a_retention_rule_callback() {
    let node = node_with(NodeInitOptions {
        retention_rule: Some(floor_for_logs),
        retention_rule_context: 9,
        ..NodeInitOptions::default()
    });
    let rule = NODES
        .get(node, |state| {
            state.database.options().retention_rule.clone()
        })
        .unwrap()
        .unwrap();
    let floor = rule.retention_floor("col-log").unwrap();
    assert_eq!((floor.field.as_str(), floor.floor), ("blockNumber", 100));
    assert!(rule.retention_floor("col-other").is_none());
    node_close(node);
}

#[test]
fn an_event_subscription_delivers_the_events_it_names() {
    use crate::subscription::{close_subscription, create_event_subscription, poll_subscription};
    let node = node();
    let names = cstr(r#"["update", "p2p-no-peers"]"#);
    let subscription = unsafe { create_event_subscription(node, names.as_ptr()) };
    assert_eq!(subscription.status, 0);
    let alice = add(node, "Alice", 40);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    let event = loop {
        let polled = poll_subscription(subscription.subscription_handle);
        if polled.status == 0 {
            let text = unsafe { CStr::from_ptr(polled.value) }
                .to_string_lossy()
                .into_owned();
            unsafe { defra_free_string(polled.value) };
            break serde_json::from_str::<Value>(&text).unwrap();
        }
        assert!(std::time::Instant::now() < deadline, "no update event");
        std::thread::sleep(std::time::Duration::from_millis(10));
    };
    assert_eq!(event["type"], "update");
    assert_eq!(event["doc_id"], alice);
    assert_eq!(event["is_relay"], false);
    let _ = close_subscription(subscription.subscription_handle);

    let unknown = cstr(r#"["nope"]"#);
    let refused = unsafe { create_event_subscription(node, unknown.as_ptr()) };
    assert_eq!(refused.status, 1);
    unsafe { defra_free_string(refused.error) };
    node_close(node);
}

#[test]
fn batch_collection_returns_the_composites_a_tagged_write_produces() {
    let node = node();
    let session = cstr("collect-without-signing");
    take(unsafe { crate::batch::batch_collect_start(session.as_ptr(), null()) }).unwrap();
    let request = cstr(
        r#"mutation { add_User(input: [{name: "Alice", age: 40}, {name: "Bob", age: 7}]) { _docID } }"#,
    );
    take(unsafe {
        exec_request(
            node,
            null(),
            null(),
            request.as_ptr(),
            null(),
            null(),
            session.as_ptr(),
        )
    })
    .unwrap();
    let cids = take(unsafe { crate::batch::batch_collect_take(session.as_ptr()) }).unwrap();
    assert_eq!(
        cids.as_array().unwrap().len(),
        2,
        "one composite per document: {cids}"
    );
    let error = take(unsafe { crate::batch::batch_collect_take(session.as_ptr()) }).unwrap_err();
    assert_eq!(error, "no batch session active");
    node_close(node);
}

#[cfg(feature = "libp2p")]
fn p2p_node(options: NodeInitOptions) -> Result<usize, String> {
    assert!(crate::runtime::init_runtime());
    let listen = cstr("/ip4/127.0.0.1/tcp/0");
    let result = unsafe { crate::p2p::new_node_with_p2p(options, listen.as_ptr()) };
    match result.status {
        0 => Ok(result.node_ptr),
        _ => {
            let error = unsafe { CStr::from_ptr(result.error) }
                .to_string_lossy()
                .into_owned();
            unsafe { defra_free_string(result.error) };
            Err(error)
        }
    }
}

/// The node's peer ID and its listen addresses.
#[cfg(feature = "libp2p")]
fn peer(node: usize) -> (String, Vec<String>) {
    let info = take(unsafe { crate::p2p::p2p_peer_info(node, null()) }).unwrap();
    let addrs: Vec<String> = serde_json::from_value(info).unwrap();
    let id = addrs[0].rsplit("/p2p/").next().unwrap().to_owned();
    (id, addrs)
}

#[cfg(feature = "libp2p")]
#[test]
fn a_supplied_peer_key_is_the_node_peer_id_on_every_start() {
    use crypto::Key;
    let key = crypto::generate_ed25519().unwrap().raw().to_vec();
    assert_eq!(key.len(), 64);
    let with_key = || NodeInitOptions {
        p2p_private_key: key.as_ptr(),
        p2p_private_key_len: key.len(),
        ..NodeInitOptions::default()
    };
    let first = p2p_node(with_key()).unwrap();
    let second = p2p_node(with_key()).unwrap();
    let other = p2p_node(NodeInitOptions::default()).unwrap();
    assert_eq!(peer(first).0, peer(second).0);
    assert_ne!(peer(first).0, peer(other).0);
    for node in [first, second, other] {
        node_close(node);
    }
    let short = [7u8; 32];
    let refused = p2p_node(NodeInitOptions {
        p2p_private_key: short.as_ptr(),
        p2p_private_key_len: short.len(),
        ..NodeInitOptions::default()
    });
    assert!(refused.unwrap_err().contains("64 bytes"));
}

#[cfg(feature = "libp2p")]
#[test]
fn every_listen_address_and_the_resource_limits_reach_the_host() {
    let addrs = cstr(r#"["/ip4/127.0.0.1/tcp/0", "/ip4/127.0.0.2/tcp/0"]"#);
    let node = p2p_node(NodeInitOptions {
        p2p_listen_addrs_json: addrs.as_ptr(),
        p2p_resource_memory_mib: 256,
        p2p_resource_file_descriptors: 512,
        p2p_max_streams_per_peer: 64,
        p2p_disable_relay: 1,
        ..NodeInitOptions::default()
    })
    .unwrap();
    let (_, listening) = peer(node);
    assert!(
        listening.iter().any(|a| a.starts_with("/ip4/127.0.0.1/")),
        "{listening:?}"
    );
    assert!(
        listening.iter().any(|a| a.starts_with("/ip4/127.0.0.2/")),
        "{listening:?}"
    );
    node_close(node);
    let bad = cstr("not json");
    let refused = p2p_node(NodeInitOptions {
        p2p_listen_addrs_json: bad.as_ptr(),
        ..NodeInitOptions::default()
    });
    assert!(refused.unwrap_err().contains("p2p_listen_addrs_json"));
}

#[test]
fn waiting_on_a_subscription_parks_until_an_event_or_the_timeout() {
    use crate::subscription::{close_subscription, create_event_subscription, wait_subscription};
    let node = node();
    let names = cstr(r#"["update"]"#);
    let subscription = unsafe { create_event_subscription(node, names.as_ptr()) };
    assert_eq!(subscription.status, 0);
    let handle = subscription.subscription_handle;

    let started = std::time::Instant::now();
    let idle = wait_subscription(handle, 200);
    assert_eq!(idle.status, 2);
    assert!(started.elapsed() >= std::time::Duration::from_millis(190));

    let waiter = std::thread::spawn(move || {
        let waited = wait_subscription(handle, 10_000);
        assert_eq!(waited.status, 0);
        let text = unsafe { CStr::from_ptr(waited.value) }
            .to_string_lossy()
            .into_owned();
        unsafe { defra_free_string(waited.value) };
        serde_json::from_str::<Value>(&text).unwrap()
    });
    let alice = add(node, "Alice", 40);
    let event = waiter.join().unwrap();
    assert_eq!(event["type"], "update");
    assert_eq!(event["doc_id"], alice);

    let _ = close_subscription(handle);
    let closed = wait_subscription(handle, 10);
    assert_eq!(closed.status, 1);
    unsafe { defra_free_string(closed.error) };
    node_close(node);
}
