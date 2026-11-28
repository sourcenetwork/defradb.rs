//! A node with P2P on and no peer accepts every write: creates, a batched
//! create and an update all succeed and read back, as on Go (#2102). A peer
//! subscribed to the collection topic alone receives them (#3388).

use integration_test::actions::{
    Actions, AddCollection, AddCollectionSubscription, AddDoc, ConnectPeers, NewNode, Request,
    WaitForPeersEvents, WaitForSync,
};
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::{json, Value};

fn on_db0() -> Common {
    Common {
        node: Some("db0".into()),
        ..Default::default()
    }
}

fn request(query: &str, expected: Value) -> Actions {
    Request {
        request: query.into(),
        expected_data: expected,
        common: on_db0(),
        ..Default::default()
    }
    .into()
}

fn request_on_db1(query: &str, expected: Value) -> Actions {
    Request {
        request: query.into(),
        expected_data: expected,
        common: on_db1(),
        ..Default::default()
    }
    .into()
}

#[test]
#[mantra_macros::req_test("defra.p2p.peerless_broadcast")]
fn writes_on_a_networked_node_with_no_peer_succeed() {
    TestCase {
        id: integration_test::scenario_id!(),
        ..TestCase::from_actions(vec![
            NewNode {
                network: true,
                common: on_db0(),
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: "type User { name: String  age: Int }".into(),
                common: on_db0(),
                ..Default::default()
            }
            .into(),
            request(
                r#"mutation {
                    a: add_User(input: {name: "Ada", age: 36}) { name }
                    b: add_User(input: {name: "Bob", age: 41}) { name }
                }"#,
                json!({ "a": [{ "name": "Ada" }], "b": [{ "name": "Bob" }] }),
            ),
            request(
                r#"mutation { add_User(input: {name: "Cy", age: 7}) { name } }"#,
                json!({ "add_User": [{ "name": "Cy" }] }),
            ),
            request(
                r#"mutation { update_User(filter: {name: {_eq: "Ada"}}, input: {age: 37}) { name age } }"#,
                json!({ "update_User": [{ "name": "Ada", "age": 37 }] }),
            ),
            request(
                "query { User(order: {name: ASC}) { name age } }",
                json!({ "User": [
                    { "name": "Ada", "age": 37 },
                    { "name": "Bob", "age": 41 },
                    { "name": "Cy", "age": 7 },
                ] }),
            ),
        ])
    }
    .execute();
}

fn on_db1() -> Common {
    Common {
        node: Some("db1".into()),
        ..Default::default()
    }
}

/// The document topic has no peer: the writer announces the write on the
/// collection topic, the second node receives it, and the write is not a
/// failure. This is the inverse of a write whose collection topic has no
/// peer.
#[test]
#[mantra_macros::req_test("defra.p2p.peerless_broadcast")]
fn writes_reach_a_peer_subscribed_to_the_collection_topic_alone() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NewNode {
                network: true,
                common: on_db0(),
                ..Default::default()
            }
            .into(),
            NewNode {
                network: true,
                common: on_db1(),
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: "type User { name: String  age: Int }".into(),
                common: on_db0(),
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: "type User { name: String  age: Int }".into(),
                common: on_db1(),
                ..Default::default()
            }
            .into(),
            ConnectPeers {
                target: "db0".into(),
                common: on_db1(),
                ..Default::default()
            }
            .into(),
            AddCollectionSubscription {
                collections: vec!["User".into()],
                common: on_db1(),
                ..Default::default()
            }
            .into(),
            WaitForPeersEvents {
                start: true,
                common: on_db0(),
                ..Default::default()
            }
            .into(),
            WaitForPeersEvents {
                start: true,
                common: on_db1(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Ada", "age": 36 })),
                common: Common {
                    node: Some("db0".into()),
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_id: Some(Text::reference("doc0_0")),
                common: Common {
                    node: Some("db0".into()),
                    capture: captures([("head0_0", "/cid")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_id: Some(Text::reference("doc0_0")),
                cid: Some(Text::reference("head0_0")),
                expected_data: json!({}),
                common: on_db1(),
                ..Default::default()
            }
            .into(),
            request_on_db1(
                "query { User { name age } }",
                json!({ "User": [{ "name": "Ada", "age": 36 }] }),
            )
            .into(),
        ],
        without_db: true,
        ..Default::default()
    }
    .execute();
}
