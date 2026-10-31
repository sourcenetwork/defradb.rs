//! A mixed cluster: a Rust node and a Go node built from the Shinzo pin
//! replicate documents both ways through replicators (#2443). The Go node
//! runs from `DEFRA_GO_SHINZO_BINARY` whatever the test's client.

use integration_test::actions::{AddCollection, AddReplicator, ConnectPeers, NewNode, Request};
use integration_test::scenario::{Common, Eventually};
use integration_test::test_case::TestCase;
use serde_json::{json, Value};

const RUST: &str = "db0";
const GO: &str = "db1";

fn on(node: &str) -> Common {
    Common {
        node: Some(node.into()),
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

/// `node` eventually holds exactly `users`, as (name, age).
fn holds(node: &str, users: &[(&str, i64)]) -> integration_test::actions::Actions {
    let users: Vec<Value> = users
        .iter()
        .map(|(name, age)| json!({"name": name, "age": age}))
        .collect();
    Request {
        request: "query { User(order: {name: ASC}) { name age } }".into(),
        expected_data: json!({ "User": users }),
        common: Common {
            node: Some(node.into()),
            eventually: Some(Eventually {
                timeout_millis: Some(30_000),
                ..Default::default()
            }),
            ..Default::default()
        },
        ..Default::default()
    }
    .into()
}

#[test]
fn rust_and_shinzo_replicators_sync_both_ways() {
    let mut actions = vec![
        NewNode {
            network: true,
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
    }
    actions.extend([
        ConnectPeers {
            target: GO.into(),
            common: on(RUST),
            ..Default::default()
        }
        .into(),
        AddReplicator {
            target: GO.into(),
            common: on(RUST),
            ..Default::default()
        }
        .into(),
        AddReplicator {
            target: RUST.into(),
            common: on(GO),
            ..Default::default()
        }
        .into(),
        mutate(
            RUST,
            r#"mutation { add_User(input: {name: "rust", age: 1}) { name } }"#,
        ),
        mutate(
            GO,
            r#"mutation { add_User(input: {name: "go", age: 2}) { name } }"#,
        ),
        holds(RUST, &[("go", 2), ("rust", 1)]),
        holds(GO, &[("go", 2), ("rust", 1)]),
    ]);
    TestCase::from_actions(actions).execute();
}
