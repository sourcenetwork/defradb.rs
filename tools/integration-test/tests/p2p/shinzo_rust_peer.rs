//! A Rust node speaking Go Shinzo's wire fetches from a Rust peer that has
//! it off without waiting out a timeout: every Rust node answers Go's CAR
//! request shape, whatever its own switch says (#2438). The deadline is
//! under the 10 s CAR reply timeout, so a peer that stayed silent fails it.

use integration_test::actions::shared::{EventType, Topic};
use integration_test::actions::{
    AddCollection, AddCollectionSubscription, ConnectPeers, GetCollections, NewNode, Request,
    WaitForPeersEvents,
};
use integration_test::scenario::{captures, Common, Eventually, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

const SHINZO: &str = "db0";
const PLAIN: &str = "db1";

fn on(node: &str) -> Common {
    Common {
        node: Some(node.into()),
        ..Default::default()
    }
}

#[test]
fn a_shinzo_node_fetches_from_a_rust_peer_with_the_wire_off() {
    let mut actions = vec![
        NewNode {
            network: true,
            shinzo_wire: true,
            common: on(SHINZO),
            ..Default::default()
        }
        .into(),
        NewNode {
            network: true,
            rust: true,
            common: on(PLAIN),
            ..Default::default()
        }
        .into(),
    ];
    for node in [SHINZO, PLAIN] {
        actions.push(
            AddCollection {
                sdl: "type User { name: String  age: Int }".into(),
                common: on(node),
                ..Default::default()
            }
            .into(),
        );
        actions.push(
            AddCollectionSubscription {
                collections: vec!["User".into()],
                common: on(node),
                ..Default::default()
            }
            .into(),
        );
    }
    actions.extend([
        GetCollections {
            collection_name: Some("User".into()),
            filtered: true,
            common: Common {
                node: Some(PLAIN.into()),
                capture: captures([("collection", "/collections/0/CollectionID")]),
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        WaitForPeersEvents {
            start: true,
            common: on(SHINZO),
            ..Default::default()
        }
        .into(),
        ConnectPeers {
            target: PLAIN.into(),
            common: on(SHINZO),
            ..Default::default()
        }
        .into(),
        WaitForPeersEvents {
            topics: Some(vec![Topic {
                topic: Text::reference("collection"),
                peers: vec![PLAIN.into()],
            }]),
            event_type: Some(EventType::Joined),
            millis: Some(10_000),
            common: on(SHINZO),
            ..Default::default()
        }
        .into(),
        Request {
            request: r#"mutation { create_User(input: {name: "Ada", age: 36}) { name } }"#.into(),
            common: on(PLAIN),
            ..Default::default()
        }
        .into(),
        Request {
            request: "query { User { name age } }".into(),
            expected_data: json!({ "User": [{ "name": "Ada", "age": 36 }] }),
            common: Common {
                node: Some(SHINZO.into()),
                eventually: Some(Eventually {
                    timeout_millis: Some(5_000),
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
