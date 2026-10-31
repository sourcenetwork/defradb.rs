//! A gossip burst from one peer that goes over the receiver's rate bucket and
//! its pending-DAG quota loses nothing (#2489): gossip never re-announces a
//! write-once document, so every refused head is kept durably and fetched
//! once a slot frees. Past the peer's deferral budget a refused head is
//! dropped, and the node reports each one as a `p2p-gossip-head-dropped`
//! event naming the peer, so an application can resync it.

use integration_test::actions::shared::{EventType, Topic};
use integration_test::actions::{
    Actions, AddCollection, AddCollectionSubscription, AdmissionSpec, ConnectPeers, GetCollections,
    NewNode, Request, WaitForPeersEvents,
};
use integration_test::scenario::{captures, Common, Eventually, Text};
use integration_test::test_case::TestCase;
use serde_json::{json, Value};

const RECEIVER: &str = "db0";
const SENDER: &str = "db1";
const SDL: &str = "type Log { address: String  blockNumber: Int }";

fn on(node: &str) -> Common {
    Common {
        node: Some(node.into()),
        ..Default::default()
    }
}

/// A receiver with `admission` that sends no CAR requests, a sender, both on
/// the Log topic, then one mutation of `logs` Logs on the sender.
fn burst(admission: AdmissionSpec, logs: i64) -> Vec<Actions> {
    let mut actions: Vec<Actions> = vec![
        NewNode {
            network: true,
            no_car_requests: true,
            admission: Some(admission),
            common: on(RECEIVER),
            ..Default::default()
        }
        .into(),
        NewNode {
            network: true,
            rust: true,
            common: on(SENDER),
            ..Default::default()
        }
        .into(),
    ];
    for node in [RECEIVER, SENDER] {
        actions.push(
            AddCollection {
                sdl: SDL.into(),
                common: on(node),
                ..Default::default()
            }
            .into(),
        );
        actions.push(
            AddCollectionSubscription {
                collections: vec!["Log".into()],
                common: on(node),
                ..Default::default()
            }
            .into(),
        );
    }
    let inputs: Vec<String> = (0..logs)
        .map(|n| format!(r#"{{address: "a{n}", blockNumber: {n}}}"#))
        .collect();
    actions.extend([
        GetCollections {
            collection_name: Some("Log".into()),
            filtered: true,
            common: Common {
                node: Some(SENDER.into()),
                capture: captures([("collection", "/collections/0/CollectionID")]),
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        WaitForPeersEvents {
            start: true,
            common: on(RECEIVER),
            ..Default::default()
        }
        .into(),
        ConnectPeers {
            target: SENDER.into(),
            common: on(RECEIVER),
            ..Default::default()
        }
        .into(),
        WaitForPeersEvents {
            topics: Some(vec![Topic {
                topic: Text::reference("collection"),
                peers: vec![SENDER.into()],
            }]),
            event_type: Some(EventType::Joined),
            millis: Some(10_000),
            common: on(RECEIVER),
            ..Default::default()
        }
        .into(),
        Request {
            request: format!(
                "mutation {{ add_Log(input: [{}]) {{ blockNumber }} }}",
                inputs.join(", ")
            )
            .into(),
            common: on(SENDER),
            ..Default::default()
        }
        .into(),
    ]);
    actions
}

/// The receiver's Logs, waited for until they are `expected`.
fn receiver_holds(expected: Value) -> Actions {
    Request {
        request: "query { Log(order: {blockNumber: ASC}) { blockNumber } }".into(),
        expected_data: json!({ "Log": expected }),
        common: Common {
            node: Some(RECEIVER.into()),
            eventually: Some(Eventually {
                timeout_millis: Some(90_000),
                ..Default::default()
            }),
            ..Default::default()
        },
        ..Default::default()
    }
    .into()
}

#[test]
fn a_gossip_burst_over_the_admission_limits_loses_no_document() {
    // Far over the receiver's 5-token burst and its 2-slot per-peer quota.
    const LOGS: i64 = 60;
    let mut actions = burst(
        AdmissionSpec {
            // One quarter of this is the sender's quota: two slots.
            max_pending_dags: 8,
            rate_limit_burst: 5,
            rate_limit_rate: 1.0,
            deferral_peer_bytes: None,
        },
        LOGS,
    );
    let expected: Vec<_> = (0..LOGS).map(|n| json!({ "blockNumber": n })).collect();
    actions.push(receiver_holds(json!(expected)));
    TestCase::from_actions(actions).execute();
}

#[test]
fn a_gossip_head_past_the_deferral_budget_is_reported() {
    // One token and no refill: the first head is admitted, every other one is
    // refused for rate, and with no deferral budget each is dropped.
    const LOGS: i64 = 10;
    let mut actions = burst(
        AdmissionSpec {
            max_pending_dags: 8,
            rate_limit_burst: 1,
            rate_limit_rate: 0.0,
            deferral_peer_bytes: Some(0),
        },
        LOGS,
    );
    actions.extend([
        WaitForPeersEvents {
            gossip_dropped: Some((LOGS - 1) as usize),
            millis: Some(30_000),
            expected_data: json!({ "dropped": LOGS - 1, "peers": 1 }),
            common: on(RECEIVER),
            ..Default::default()
        }
        .into(),
        Request {
            request: "query { COUNT(Log: {}) }".into(),
            expected_data: json!({ "COUNT": 1 }),
            common: Common {
                node: Some(RECEIVER.into()),
                eventually: Some(Eventually {
                    timeout_millis: Some(30_000),
                    ..Default::default()
                }),
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
    ]);
    TestCase::from_actions(actions).execute();
}
