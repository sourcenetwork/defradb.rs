//! A Rust node speaking Go Shinzo's wire and a Go `shinzo-develop` node
//! replicate creates, updates and deletes both ways over collection
//! subscriptions (#2438). Go publishes a collection's updates in batches, so
//! the three documents Go creates in one request reach Rust as one batched
//! push; Rust asks Go for each head's CAR over Go's car channel, and Go asks
//! Rust the same way.
//!
//! Needs `DEFRA_GO_SHINZO_BINARY`, a Go `defradb` built from `shinzo-develop`.

use integration_test::actions::shared::{EventType, Topic};
use integration_test::actions::{
    AddCollection, AddCollectionSubscription, ConnectPeers, GetCollections, NewNode, Request,
    WaitForPeersEvents,
};
use integration_test::scenario::{captures, Common, Eventually, Text};
use integration_test::test_case::TestCase;
use serde_json::{json, Value};

const RUST: &str = "db0";
const GO: &str = "db1";
const USERS: &str = "query { User(order: {name: ASC}) { name age } }";

fn on(node: &str) -> Common {
    Common {
        node: Some(node.into()),
        ..Default::default()
    }
}

fn eventually_on(node: &str) -> Common {
    Common {
        node: Some(node.into()),
        eventually: Some(Eventually {
            timeout_millis: Some(30_000),
            ..Default::default()
        }),
        ..Default::default()
    }
}

fn mutate(node: &str, request: &str) -> integration_test::actions::Actions {
    Request {
        request: request.into(),
        common: on(node),
        ..Default::default()
    }
    .into()
}

/// Both nodes eventually hold exactly `users`, as (name, age).
fn both_hold(users: &[(&str, i64)]) -> Vec<integration_test::actions::Actions> {
    let expected: Vec<Value> = users
        .iter()
        .map(|(name, age)| json!({"name": name, "age": age}))
        .collect();
    [RUST, GO]
        .into_iter()
        .map(|node| {
            Request {
                request: USERS.into(),
                expected_data: json!({ "User": expected }),
                common: eventually_on(node),
                ..Default::default()
            }
            .into()
        })
        .collect()
}

#[test]
fn rust_and_shinzo_nodes_replicate_on_the_shinzo_wire() {
    let mut actions = vec![
        NewNode {
            network: true,
            shinzo_wire: true,
            common: on(RUST),
            ..Default::default()
        }
        .into(),
        NewNode {
            network: true,
            go_shinzo: true,
            common: on(GO),
            ..Default::default()
        }
        .into(),
    ];
    for node in [RUST, GO] {
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
        // Both nodes derive the same collection ID; Go's HTTP API can look
        // it up by name under every client.
        GetCollections {
            collection_name: Some("User".into()),
            filtered: true,
            common: Common {
                node: Some(GO.into()),
                capture: captures([("collection", "/collections/0/CollectionID")]),
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        WaitForPeersEvents {
            start: true,
            common: on(RUST),
            ..Default::default()
        }
        .into(),
        ConnectPeers {
            target: GO.into(),
            common: on(RUST),
            ..Default::default()
        }
        .into(),
        // Updates are published on the collection's topic; wait until Go is on it.
        WaitForPeersEvents {
            topics: Some(vec![Topic {
                topic: Text::reference("collection"),
                peers: vec![GO.into()],
            }]),
            event_type: Some(EventType::Joined),
            millis: Some(10_000),
            common: on(RUST),
            ..Default::default()
        }
        .into(),
        // One Go request, three documents: Go's batcher pushes them together.
        mutate(
            GO,
            r#"mutation { add_User(input: [
                {name: "go-a", age: 1}, {name: "go-b", age: 2}, {name: "go-c", age: 3}
            ]) { name } }"#,
        ),
        mutate(
            RUST,
            r#"mutation { add_User(input: [
                {name: "rust-a", age: 4}, {name: "rust-b", age: 5}
            ]) { name } }"#,
        ),
    ]);
    actions.extend(both_hold(&[
        ("go-a", 1),
        ("go-b", 2),
        ("go-c", 3),
        ("rust-a", 4),
        ("rust-b", 5),
    ]));
    actions.extend([
        mutate(
            GO,
            r#"mutation { update_User(filter: {name: {_eq: "go-a"}}, input: {age: 10}) { name } }"#,
        ),
        mutate(
            RUST,
            r#"mutation { update_User(filter: {name: {_eq: "rust-a"}}, input: {age: 40}) { name } }"#,
        ),
    ]);
    actions.extend(both_hold(&[
        ("go-a", 10),
        ("go-b", 2),
        ("go-c", 3),
        ("rust-a", 40),
        ("rust-b", 5),
    ]));
    actions.extend([
        mutate(
            GO,
            r#"mutation { delete_User(filter: {name: {_eq: "go-b"}}) { name } }"#,
        ),
        mutate(
            RUST,
            r#"mutation { delete_User(filter: {name: {_eq: "rust-b"}}) { name } }"#,
        ),
    ]);
    actions.extend(both_hold(&[("go-a", 10), ("go-c", 3), ("rust-a", 40)]));
    TestCase::from_actions(actions).execute();
}
