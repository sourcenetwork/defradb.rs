//! A history prune beside peers: a concurrent head below the window stays,
//! and pruned history a peer sends back is taken again by the next prune.

use integration_test::actions::{
    Actions, AddCollection, AddCollectionSubscription, AddDoc, ConnectPeers, DisconnectPeers,
    NewNode, PruneHistory, Request, UpdateDoc, WaitForPeersEvents,
};
use integration_test::scenario::{captures, Common, Eventually, Text};
use serde_json::{json, Value};

use super::rust_only;

fn on(node: &str) -> Common {
    Common {
        node: Some(node.into()),
        ..Default::default()
    }
}

/// `db0` and `db1`, networked and subscribed to `User`, which keeps `depth`
/// heights.
fn nodes(depth: u64) -> Vec<Actions> {
    let sdl = format!("type User @retention(maxDepth: {depth}) {{ name: String age: Int }}");
    let mut actions = Vec::new();
    for node in ["db0", "db1"] {
        actions.push(
            NewNode {
                network: true,
                common: on(node),
                ..Default::default()
            }
            .into(),
        );
        actions.push(
            AddCollection {
                sdl: sdl.as_str().into(),
                common: on(node),
                ..Default::default()
            }
            .into(),
        );
        actions.push(
            WaitForPeersEvents {
                start: true,
                common: on(node),
                ..Default::default()
            }
            .into(),
        );
    }
    actions.push(connect());
    for node in ["db0", "db1"] {
        actions.push(
            AddCollectionSubscription {
                collections: vec!["User".into()],
                common: on(node),
                ..Default::default()
            }
            .into(),
        );
    }
    actions
}

fn connect() -> Actions {
    ConnectPeers {
        target: "db1".into(),
        common: on("db0"),
        ..Default::default()
    }
    .into()
}

fn disconnect() -> Actions {
    DisconnectPeers {
        targets: vec!["db1".into()],
        common: on("db0"),
        ..Default::default()
    }
    .into()
}

fn create() -> Actions {
    AddDoc {
        collection: "User".into(),
        doc_map: Some(json!({ "name": "John", "age": 0 })),
        common: Common {
            retry: true,
            capture: captures([("john", "/docID")]),
            ..on("db0")
        },
        ..Default::default()
    }
    .into()
}

fn update(node: &str, age: i64) -> Actions {
    UpdateDoc {
        collection: "User".into(),
        doc_id: Text::reference("john"),
        doc_map: json!({ "age": age }),
        expected_data: json!({}),
        common: Common {
            retry: true,
            ..on(node)
        },
        ..Default::default()
    }
    .into()
}

fn prune(node: &str, expected: usize) -> Actions {
    PruneHistory {
        collection: "User".into(),
        expected_count: Some(expected),
        common: on(node),
        ..Default::default()
    }
    .into()
}

/// The heights of John's composites on `node`, highest first, once they
/// are what `expected` says.
fn composites(node: &str, expected: &[u64]) -> Actions {
    heights(node, "_C", expected)
}

fn heights(node: &str, field: &str, expected: &[u64]) -> Actions {
    let rows: Vec<Value> = expected.iter().map(|h| json!({ "height": h })).collect();
    Request {
        request: Text::concat([
            Text::from(r#"query { _commits(docID: ""#),
            Text::reference("john"),
            Text::from(format!(
                r#"", filter: {{fieldName: {{_eq: "{field}"}}}}, order: {{height: DESC}}) {{ height }} }}"#
            )),
        ]),
        expected_data: json!({ "_commits": rows }),
        common: Common {
            eventually: Some(Eventually::default()),
            ..on(node)
        },
        ..Default::default()
    }
    .into()
}

fn age(node: &str, age: i64) -> Actions {
    Request {
        request: "query { User { name age } }".into(),
        expected_data: json!({ "User": [{ "name": "John", "age": age }] }),
        common: Common {
            eventually: Some(Eventually::default()),
            ..on(node)
        },
        ..Default::default()
    }
    .into()
}

/// `db1` writes a branch of its own at height 2 while cut off, then merges
/// `db0`'s line up to height 3. With `maxDepth: 1` the cutoff is height 2:
/// `db0`'s composite there goes, `db1`'s stays because it is still a head.
#[test]
#[mantra_macros::req_test("defra.retention.prune")]
fn a_concurrent_head_below_the_window_is_kept() {
    let mut actions = nodes(1);
    actions.extend([
        create(),
        age("db1", 0),
        disconnect(),
        update("db1", 999),
        update("db0", 1),
        connect(),
        update("db0", 2),
        composites("db1", &[3, 2, 2, 1]),
        prune("db1", 2),
        composites("db1", &[3, 2]),
        age("db1", 2),
    ]);
    rust_only(actions).execute();
}

/// `db1` prunes height 1 and its own height 2 while cut off; `db0` meanwhile
/// built height 2 on height 1. Back in touch, `db0`'s next update reaches
/// `db1`, which fetches the pruned height 1 back to complete the DAG. The
/// merge finds it already applied, so it is no head, and the next prune
/// takes it again, with `db0`'s height 2 below the window.
#[test]
#[mantra_macros::req_test("defra.retention.prune")]
fn pruned_history_a_peer_sends_back_goes_in_the_next_prune() {
    let mut actions = nodes(2);
    actions.extend([
        create(),
        age("db1", 0),
        disconnect(),
        update("db0", 1),
        update("db1", 10),
        update("db1", 11),
        update("db1", 12),
        prune("db1", 2),
        composites("db1", &[4, 3]),
        connect(),
        update("db0", 2),
        composites("db1", &[4, 3, 3, 2, 1]),
        prune("db1", 2),
        composites("db1", &[4, 3, 3]),
        heights("db1", "age", &[4, 3, 3]),
        heights("db1", "name", &[1]),
        age("db1", 12),
    ]);
    rust_only(actions).execute();
}
