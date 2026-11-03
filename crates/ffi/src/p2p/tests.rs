use std::time::{Duration, Instant};

use serde_json::{json, Value};

use super::*;
use crate::test_support::{
    add_collection, close, cstr, err, local_options, node_with, ok, ok_json, query,
};
use crate::types::NodeInitOptions;

/// A node listening for libp2p on a free local port.
fn p2p_node() -> usize {
    let listen = cstr("/ip4/127.0.0.1/tcp/0");
    node_with(NodeInitOptions {
        disable_p2p: 0,
        listening_addresses: listen.as_ptr(),
        enable_pub_sub: 1,
        ..local_options()
    })
}

/// The first address other peers reach `node` at.
fn address(node: usize) -> String {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let info = ok_json(defra_p2p_info(node, 0));
        if let Some(address) = info
            .as_array()
            .and_then(|a| a.first())
            .and_then(Value::as_str)
        {
            return address.to_string();
        }
        assert!(Instant::now() < deadline, "the node never listened: {info}");
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Polls `read` until it answers `want`, or fails after 20 seconds.
fn eventually(want: Value, read: impl Fn() -> Value) {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let got = read();
        if got == want {
            return;
        }
        assert!(Instant::now() < deadline, "wanted {want}, still {got}");
        std::thread::sleep(Duration::from_millis(100));
    }
}

#[test]
fn a_node_without_p2p_reports_it() {
    let node = node_with(local_options());
    let error = err(defra_p2p_info(node, 0));
    assert_eq!(error, "no p2p system configured");
    close(node);
}

#[test]
fn a_replicator_pushes_documents_to_its_peer() {
    let source = p2p_node();
    let target = p2p_node();
    for node in [source, target] {
        add_collection(node, "type User { name: String }");
    }

    let collections = cstr("User");
    let target_address = cstr(&address(target));
    ok(unsafe {
        defra_p2p_replicator_add(source, collections.as_ptr(), target_address.as_ptr(), 0)
    });
    let replicators = ok_json(defra_p2p_replicator_list(source, 0));
    assert_eq!(
        replicators.as_array().map(Vec::len),
        Some(1),
        "{replicators}"
    );
    assert!(
        replicators[0]["Addresses"].is_array(),
        "Go's client.Replicator shape: {replicators}"
    );

    query(
        source,
        r#"mutation { add_User(input: {name: "replicated"}) { _docID } }"#,
    );
    eventually(json!([{"name": "replicated"}]), || {
        query(target, "{ User { name } }")["data"]["User"].clone()
    });

    close(source);
    close(target);
}

#[test]
fn p2p_collections_are_added_listed_and_removed() {
    let node = p2p_node();
    let created = add_collection(node, "type User { name: String }");
    let collection_id = created[0]["CollectionID"]
        .as_str()
        .unwrap_or("User")
        .to_string();

    let collections = cstr("User");
    ok(unsafe { defra_p2p_collection_add(node, collections.as_ptr(), 0) });
    let listed = ok_json(defra_p2p_collection_list(node, 0));
    assert_eq!(
        listed.as_array().map(Vec::len),
        Some(1),
        "{listed} ({collection_id})"
    );

    ok(unsafe { defra_p2p_collection_delete(node, collections.as_ptr(), 0) });
    assert_eq!(ok_json(defra_p2p_collection_list(node, 0)), json!([]));
    close(node);
}

#[test]
fn peers_connect_and_disconnect() {
    let first = p2p_node();
    let second = p2p_node();

    let second_address = cstr(&address(second));
    ok(unsafe { defra_p2p_connect(first, second_address.as_ptr(), 0) });
    let peers = ok_json(defra_p2p_peer_list(first, 0));
    assert_eq!(peers.as_array().map(Vec::len), Some(1), "{peers}");

    ok(unsafe { defra_p2p_disconnect(first, second_address.as_ptr(), 0) });
    close(first);
    close(second);
}

#[test]
fn a_node_without_p2p_has_no_shareable_address_yet() {
    let node = node_with(local_options());
    assert_eq!(ok(defra_mobile_shareable_address(node, 0)), "null");
    close(node);
}

#[test]
fn the_shareable_address_is_a_string_or_null() {
    let node = p2p_node();
    let address = ok_json(defra_mobile_shareable_address(node, 0));
    assert!(address.is_string() || address.is_null(), "{address}");
    close(node);
}

#[test]
fn an_invalid_handle_has_no_shareable_address() {
    assert_eq!(
        err(defra_mobile_shareable_address(usize::MAX, 0)),
        crate::target::ERR_INVALID_HANDLE
    );
}

#[test]
fn the_sync_status_reports_the_push_backlog() {
    let node = p2p_node();
    let status = ok_json(defra_mobile_sync_status(node, 0));
    assert!(status.get("push_backlog").is_some(), "{status}");
    close(node);
}

#[test]
fn the_sync_status_needs_p2p() {
    let node = node_with(local_options());
    assert_eq!(err(defra_mobile_sync_status(node, 0)), ERR_NO_P2P);
    close(node);
}

#[test]
fn peer_details_answer_for_a_peer_and_refuse_a_bad_id() {
    let node = p2p_node();
    let peer = "12D3KooWD3eckifWpRn9wQpMG9R9hX3sD158z7EqHWmweQAJU5SA";
    let peer_id = cstr(peer);
    let details = ok_json(unsafe { defra_mobile_peer_details(node, peer_id.as_ptr(), 0) });
    assert_eq!(details["peer_id"], peer, "{details}");
    assert_eq!(details["connected"], false, "{details}");
    assert!(details.get("heads_pushed").is_some(), "{details}");

    let invalid = cstr("/ip4/127.0.0.1");
    err(unsafe { defra_mobile_peer_details(node, invalid.as_ptr(), 0) });
    close(node);
}

#[test]
fn a_network_change_is_accepted_with_p2p_and_refused_without() {
    let node = p2p_node();
    ok(defra_mobile_notify_network_change(node, 0));
    close(node);

    let node = node_with(local_options());
    assert_eq!(err(defra_mobile_notify_network_change(node, 0)), ERR_NO_P2P);
    close(node);
}

#[test]
fn a_bad_timeout_fails_before_syncing() {
    let node = p2p_node();
    let collection = cstr("User");
    let doc_ids = cstr("");
    let timeout = cstr("30");
    let error = err(unsafe {
        defra_p2p_document_sync(
            node,
            collection.as_ptr(),
            doc_ids.as_ptr(),
            timeout.as_ptr(),
            0,
        )
    });
    assert!(error.contains("missing unit"), "{error}");
    close(node);
}
