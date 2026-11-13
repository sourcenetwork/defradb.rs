//! Event subscriptions through the C API: what they deliver, the names they
//! take, and who node access control lets subscribe.

use std::ffi::CString;
use std::ptr;
use std::time::Duration;

use serde_json::Value;

use super::{defra_event_subscribe, parse_names};
use crate::identities::{defra_identity_free, defra_identity_new};
use crate::subscription::{defra_subscription_close, defra_subscription_poll};
use crate::test_support::{
    add_collection, close, cstr, err, local_options, node, node_with, ok, query, take, wait_for,
};
use crate::types::NodeInitOptions;

/// Subscribes to `names` on `node` as `identity` and answers with the ID.
fn subscribe(node: usize, names: &str, identity: usize) -> CString {
    let names = cstr(names);
    let (status, id) = take(unsafe { defra_event_subscribe(node, names.as_ptr(), identity) });
    assert_eq!(status, 2, "a subscription answers with status 2: {id:?}");
    cstr(&id.expect("a subscription answers with its ID"))
}

/// Polls until an event arrives, or fails after 10 seconds.
fn next(id: &CString) -> Value {
    wait_for(Duration::from_secs(10), || {
        match take(unsafe { defra_subscription_poll(id.as_ptr()) }) {
            (0, Ok(json)) => Ok(serde_json::from_str(&json).expect("an event is JSON")),
            (2, _) => Err("no event arrived".to_string()),
            (status, outcome) => panic!("poll failed with status {status}: {outcome:?}"),
        }
    })
}

fn identity() -> usize {
    let result = unsafe { defra_identity_new(ptr::null()) };
    assert_eq!(result.status, 0);
    result.identity_ptr
}

#[test]
fn an_update_event_carries_its_document_and_block() {
    let node = node();
    add_collection(node, "type User { name: String }");
    let id = subscribe(node, "update", 0);

    let added = query(
        node,
        r#"mutation { add_User(input: {name: "A"}) { _docID } }"#,
    );
    let doc_id = added["data"]["add_User"][0]["_docID"].clone();

    let event = next(&id);
    assert_eq!(event["name"], "update", "{event}");
    assert_eq!(event["doc_id"], doc_id, "{event}");
    assert!(
        event["cid"].as_str().is_some_and(|cid| !cid.is_empty()),
        "{event}"
    );
    assert!(
        event["block"]
            .as_str()
            .is_some_and(|block| !block.is_empty()),
        "{event}"
    );

    ok(unsafe { defra_subscription_close(id.as_ptr()) });
    err(unsafe { defra_subscription_poll(id.as_ptr()) });
    close(node);
}

#[test]
fn events_take_go_names_and_refuse_unknown_ones() {
    assert_eq!(
        parse_names("update, merge-complete").unwrap(),
        [events::EventName::Update, events::EventName::MergeComplete]
    );
    assert_eq!(parse_names("*").unwrap(), [events::EventName::WildCard]);
    assert!(parse_names("merge_complete")
        .unwrap_err()
        .contains("merge_complete"));
    assert!(parse_names("").is_err());

    let node = node();
    for names in ["", "no-such-event"] {
        let names = cstr(names);
        err(unsafe { defra_event_subscribe(node, names.as_ptr(), 0) });
    }
    close(node);
}

#[test]
fn a_subscription_needs_a_node_and_a_known_identity() {
    let names = cstr("update");
    err(unsafe { defra_event_subscribe(0, names.as_ptr(), 0) });

    let node = node();
    assert_eq!(
        err(unsafe { defra_event_subscribe(node, names.as_ptr(), 999_999) }),
        "invalid handle"
    );
    close(node);
}

#[test]
fn node_access_control_lets_only_an_identity_past_document_access_subscribe() {
    let owner = identity();
    let stranger = identity();
    let node = node_with(NodeInitOptions {
        identity_ptr: owner,
        enable_node_acp: 1,
        ..local_options()
    });
    let names = cstr("merge-complete");

    for requestor in [0, stranger] {
        let error = err(unsafe { defra_event_subscribe(node, names.as_ptr(), requestor) });
        assert!(error.contains("not authorized"), "{error}");
    }
    let id = subscribe(node, "merge-complete", owner);

    close(node);
    err(unsafe { defra_subscription_poll(id.as_ptr()) });
    defra_identity_free(owner);
    defra_identity_free(stranger);
}

/// A replicator completion carries its outcome (delivered, skipped, or a
/// replay error) under its Go name; a bare completion signal carries none.
#[test]
fn a_replicator_completion_carries_its_outcome() {
    for (skipped, error) in [(false, None), (true, None), (false, Some("replay failed"))] {
        let message =
            events::Message::replicator_completed_with_data(events::ReplicatorCompletedData {
                peer_id: "peer-a".into(),
                collections: vec!["Note".into(), "User".into()],
                skipped,
                error: error.map(str::to_owned),
            });
        assert_eq!(
            super::event_json(&message),
            serde_json::json!({
                "name": "replicator-completed",
                "peer_id": "peer-a",
                "collections": ["Note", "User"],
                "skipped": skipped,
                "error": error,
            })
        );
    }
    assert_eq!(
        super::event_json(&events::Message::replicator_completed()),
        serde_json::json!({ "name": "replicator-completed" })
    );
}

/// An action execution carries Go's wire shape of the execution.
#[test]
fn an_action_execution_carries_its_execution() {
    let execution = defra_core::ActionExecution {
        collection_id: "bafycollection".into(),
        action: defra_core::Action::TRUNCATE,
        subject: "doc".into(),
        status: defra_core::ActionStatus::ERRORED,
        reason: "failed".into(),
    };
    let event = super::event_json(&events::Message::action_execution(execution.clone()));
    assert_eq!(event["name"], "action-execution", "{event}");
    assert_eq!(
        event["action_execution"],
        serde_json::to_value(&execution).unwrap(),
        "{event}"
    );
}

/// Events the bus drops for a slow subscriber are reported, not lost
/// silently: the next event delivered is preceded by their count.
#[test]
fn events_the_bus_drops_are_reported_before_the_next_one() {
    use events::Bus;

    let bus = events::ChannelBus::with_config(events::ChannelBusConfig {
        event_buffer_size: 2,
        ..Default::default()
    });
    let subscription = bus.subscribe(&[events::EventName::ReplicatorCompleted]);
    for _ in 0..5 {
        bus.publish(events::Message::replicator_completed());
    }
    drop(bus);

    let (sender, receiver) = kovan_channel::bounded::<String>(16);
    futures_lite::future::block_on(super::forward(subscription, sender));
    let delivered: Vec<Value> = std::iter::from_fn(|| receiver.try_recv())
        .map(|json| serde_json::from_str(&json).unwrap())
        .collect();
    assert_eq!(
        delivered,
        vec![
            serde_json::json!({ "name": "events-dropped", "count": 3 }),
            serde_json::json!({ "name": "replicator-completed" }),
            serde_json::json!({ "name": "replicator-completed" }),
        ]
    );
}
