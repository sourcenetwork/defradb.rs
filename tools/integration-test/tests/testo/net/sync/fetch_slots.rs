//! A node's `p2p status` reports what holds its pending-DAG fetch slots.
//! Rust nodes only: the counters read are defradb.rs's `p2p status`.

use std::collections::BTreeMap;

use integration_test::actions::{
    Actions, AddCollection, AddCollectionSubscription, AddDoc, ConnectPeers, NewNode, Request,
    SyncCounters, UpdateDoc, WaitForPeersEvents,
};
use integration_test::scenario::{captures, Common, Eventually, Text};
use integration_test::test_case::TestCase;
use integration_test::ClientType;
use serde_json::{json, Value};

const SDL: &str = "type Users { name: String  age: Int }";

fn on(node: &str) -> Common {
    Common {
        node: Some(node.into()),
        ..Default::default()
    }
}

fn node(name: &str) -> Vec<Actions> {
    vec![
        NewNode {
            network: true,
            common: on(name),
            ..Default::default()
        }
        .into(),
        AddCollection {
            sdl: SDL.into(),
            common: on(name),
            ..Default::default()
        }
        .into(),
        WaitForPeersEvents {
            start: true,
            common: on(name),
            ..Default::default()
        }
        .into(),
    ]
}

fn create() -> Actions {
    AddDoc {
        collection: "Users".into(),
        doc_map: Some(json!({ "name": "n0", "age": 0 })),
        common: Common {
            retry: true,
            capture: captures([("doc", "/docID")]),
            ..on("db0")
        },
        ..Default::default()
    }
    .into()
}

fn update(fields: Value) -> Actions {
    UpdateDoc {
        collection: "Users".into(),
        doc_id: Text::reference("doc"),
        doc_map: fields,
        expected_data: json!({}),
        common: Common {
            retry: true,
            ..on("db0")
        },
        ..Default::default()
    }
    .into()
}

fn counters(limits: &[(&str, u64)]) -> BTreeMap<String, u64> {
    limits
        .iter()
        .map(|(name, limit)| ((*name).to_owned(), *limit))
        .collect()
}

/// `db1`, holding none of a document with a history, is told of its next
/// head, fetches the whole DAG and merges it. Its fetch slots are then all
/// free again, out of the default four.
#[test]
#[mantra_macros::req_test("defra.net.sync.fetch_slots.released")]
fn fetch_slots_are_free_once_a_fetched_dag_merges() {
    let mut actions = node("db0");
    actions.extend(node("db1"));
    actions.push(create());
    actions.extend((1..=10).map(|idx| update(json!({ "name": format!("n{idx}") }))));
    actions.push(
        ConnectPeers {
            target: "db1".into(),
            common: on("db0"),
            ..Default::default()
        }
        .into(),
    );
    for name in ["db0", "db1"] {
        actions.push(
            AddCollectionSubscription {
                collections: vec!["Users".into()],
                common: on(name),
                ..Default::default()
            }
            .into(),
        );
    }
    actions.push(update(json!({ "name": "last" })));
    actions.push(
        Request {
            request: "query { Users { name } }".into(),
            expected_data: json!({ "Users": [{ "name": "last" }] }),
            common: Common {
                eventually: Some(Eventually::default()),
                ..on("db1")
            },
            ..Default::default()
        }
        .into(),
    );
    actions.push(
        SyncCounters {
            at_least: counters(&[("car_served_cids", 1)]),
            common: on("db0"),
            ..Default::default()
        }
        .into(),
    );
    actions.push(
        SyncCounters {
            at_most: counters(&[
                ("pending_dag_fetch_slots/held", 0),
                ("pending_dag_fetch_slots/limit", 4),
            ]),
            at_least: counters(&[("pending_dag_fetch_slots/limit", 4)]),
            common: Common {
                eventually: Some(Eventually::default()),
                ..on("db1")
            },
            ..Default::default()
        }
        .into(),
    );
    TestCase {
        id: integration_test::scenario_id!(),
        actions: actions.into_iter().map(Into::into).collect(),
        client_types: Some(&[ClientType::InProcess, ClientType::RustHttp, ClientType::Cli]),
        ..Default::default()
    }
    .execute();
}
