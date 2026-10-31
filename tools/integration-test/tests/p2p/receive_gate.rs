//! A node with a retention floor or a replication filter judges a pushed
//! document before it fetches the document's blocks (#2456): a refused
//! document never arrives, and one the node keeps does. Both nodes are Rust
//! under every client.

use integration_test::actions::shared::{EventType, Topic};
use integration_test::actions::{
    AddCollection, AddCollectionSubscription, ConnectPeers, CutoffSpec, GetCollections, NewNode,
    Request, RetentionSpec, WaitForPeersEvents,
};
use integration_test::scenario::{captures, Common, Eventually, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

const JUDGING: &str = "db0";
const SENDER: &str = "db1";
const SDL: &str = "type Log { address: String  blockNumber: Int }";
/// The Log collection's content-addressed ID, the same on every node.
const LOG_ID: &str = "bafyreibjraiyakk3irmdr7nl6ijzamzemaebryyxsbfpdexz3dhmglxtae";

fn on(node: &str) -> Common {
    Common {
        node: Some(node.into()),
        ..Default::default()
    }
}

/// `judging` receives a refused Log at 50 and a kept one at 150 from a
/// sender, and ends up holding only the one at 150.
fn only_the_kept_log_arrives(judging: NewNode) {
    let mut actions = vec![
        judging.into(),
        NewNode {
            network: true,
            shinzo_wire: true,
            common: on(SENDER),
            ..Default::default()
        }
        .into(),
    ];
    for node in [JUDGING, SENDER] {
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
            common: on(JUDGING),
            ..Default::default()
        }
        .into(),
        ConnectPeers {
            target: SENDER.into(),
            common: on(JUDGING),
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
            common: on(JUDGING),
            ..Default::default()
        }
        .into(),
        Request {
            request:
                r#"mutation { create_Log(input: {address: "low", blockNumber: 50}) { address } }"#
                    .into(),
            common: on(SENDER),
            ..Default::default()
        }
        .into(),
        Request {
            request:
                r#"mutation { create_Log(input: {address: "high", blockNumber: 150}) { address } }"#
                    .into(),
            common: on(SENDER),
            ..Default::default()
        }
        .into(),
        Request {
            request: "query { Log(order: {blockNumber: ASC}) { address blockNumber } }".into(),
            expected_data: json!({ "Log": [{ "address": "high", "blockNumber": 150 }] }),
            common: Common {
                node: Some(JUDGING.into()),
                eventually: Some(Eventually {
                    timeout_millis: Some(10_000),
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

#[test]
#[mantra_macros::req_test("go.shinzo.p2p.retention.gate")]
fn a_document_below_the_retention_floor_never_arrives() {
    only_the_kept_log_arrives(NewNode {
        network: true,
        shinzo_wire: true,
        retention: Some(RetentionSpec {
            collection_id: LOG_ID.into(),
            field: "blockNumber".into(),
            floor: 100,
        }),
        common: on(JUDGING),
        ..Default::default()
    });
}

#[test]
#[mantra_macros::req_test("go.shinzo.p2p.replication_filter.after_fetch")]
fn a_document_the_filter_refuses_never_arrives() {
    only_the_kept_log_arrives(NewNode {
        network: true,
        shinzo_wire: true,
        refuse_at_or_below: Some(CutoffSpec {
            field: "blockNumber".into(),
            cutoff: 100,
        }),
        common: on(JUDGING),
        ..Default::default()
    });
}
