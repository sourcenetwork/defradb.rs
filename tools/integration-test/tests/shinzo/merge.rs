//! Documents created concurrently, and replicated documents (`go.shinzo.merge`,
//! `go.shinzo.docid`). The replication tests run two Rust nodes under every
//! client.

use integration_test::actions::shared::{EventType, Topic};
use integration_test::actions::{
    Actions, AddCollection, AddCollectionSubscription, AddDoc, ConnectPeers, GetCollections,
    NewNode, Parallel, Request, WaitForPeersEvents,
};
use integration_test::scenario::{captures, Common, Eventually, Text};
use integration_test::test_case::TestCase;
use serde_json::{json, Value};

const USERS: &str = "type User { name: String age: Int }";
const RECEIVER: &str = "db0";
const SENDER: &str = "db1";

fn on(node: Option<&str>) -> Common {
    Common {
        node: node.map(Into::into),
        ..Default::default()
    }
}

fn add(node: Option<&str>, name: &str, age: i64) -> Actions {
    AddDoc {
        collection: "User".into(),
        doc_map: Some(json!({ "name": name, "age": age })),
        common: on(node),
        ..Default::default()
    }
    .into()
}

/// 24 `User` documents, `u0` aged 0 to `u23` aged 23.
fn twenty_four() -> Vec<(String, i64)> {
    (0..24).map(|i| (format!("u{i:02}"), i)).collect()
}

fn users(rows: &[(String, i64)]) -> Value {
    Value::Array(
        rows.iter()
            .map(|(name, age)| json!({ "name": name, "age": age }))
            .collect(),
    )
}

/// `query` on `node`, retried until it returns `expected`.
fn eventually(node: Option<&str>, query: &str, expected: Value) -> Actions {
    Request {
        request: query.into(),
        expected_data: expected,
        common: Common {
            node: node.map(Into::into),
            eventually: Some(Eventually {
                timeout_millis: Some(20_000),
                ..Default::default()
            }),
            ..Default::default()
        },
        ..Default::default()
    }
    .into()
}

#[test]
#[mantra_macros::req_test("go.shinzo.docid.concurrent_creates")]
fn shinzo_concurrent_creates_all_store_their_own_documents() {
    let rows = twenty_four();
    TestCase::from_actions(vec![
        AddCollection {
            sdl: USERS.into(),
            ..Default::default()
        }
        .into(),
        Parallel {
            actions: rows
                .iter()
                .map(|(name, age)| add(None, name, *age))
                .collect(),
            ..Default::default()
        }
        .into(),
        eventually(
            None,
            "query { User(order: {name: ASC}) { name age } }",
            json!({ "User": users(&rows) }),
        ),
    ])
    .execute();
}

/// A receiver subscribed to `User` and connected to a sender, both Rust.
fn connected() -> Vec<Actions> {
    let mut actions = vec![];
    for node in [RECEIVER, SENDER] {
        actions.push(
            NewNode {
                network: true,
                shinzo_wire: true,
                common: on(Some(node)),
                ..Default::default()
            }
            .into(),
        );
    }
    for node in [RECEIVER, SENDER] {
        actions.push(
            AddCollection {
                sdl: USERS.into(),
                common: on(Some(node)),
                ..Default::default()
            }
            .into(),
        );
        actions.push(
            AddCollectionSubscription {
                collections: vec!["User".into()],
                common: on(Some(node)),
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
                node: Some(SENDER.into()),
                capture: captures([("collection", "/collections/0/CollectionID")]),
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        WaitForPeersEvents {
            start: true,
            common: on(Some(RECEIVER)),
            ..Default::default()
        }
        .into(),
        ConnectPeers {
            target: SENDER.into(),
            common: on(Some(RECEIVER)),
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
            common: on(Some(RECEIVER)),
            ..Default::default()
        }
        .into(),
    ]);
    actions
}

#[test]
#[mantra_macros::req_test("go.shinzo.merge.concurrent_creates")]
fn concurrently_created_documents_all_merge_on_the_receiver() {
    let rows = twenty_four();
    let mut actions = connected();
    actions.extend([
        Parallel {
            actions: rows
                .iter()
                .map(|(name, age)| add(Some(SENDER), name, *age))
                .collect(),
            ..Default::default()
        }
        .into(),
        eventually(
            Some(RECEIVER),
            "query { User(order: {name: ASC}) { name age } }",
            json!({ "User": users(&rows) }),
        ),
    ]);
    TestCase::from_actions(actions).execute();
}

#[test]
#[mantra_macros::req_test("go.shinzo.merge.local_ids")]
fn a_replicated_document_keeps_its_doc_id_on_the_receiver() {
    let mut actions = connected();
    actions.extend([
        // The receiver allocates its own document numbers: one it holds
        // already takes the first.
        add(Some(RECEIVER), "Local", 1),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "John", "age": 30 })),
            common: Common {
                node: Some(SENDER.into()),
                capture: captures([("john", "/docID")]),
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        eventually(
            Some(RECEIVER),
            r#"query { User(filter: {name: {_eq: "John"}}) { _docID name age } }"#,
            json!({ "User": [{ "_docID": { "$ref": "john" }, "name": "John", "age": 30 }] }),
        ),
    ]);
    TestCase::from_actions(actions).execute();
}
