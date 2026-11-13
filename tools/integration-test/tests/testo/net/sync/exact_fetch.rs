//! A DAG fetch names the heads the receiver holds, so the provider sends only
//! the blocks the receiver is missing, at any branch depth. Rust nodes only:
//! the counters read are defradb.rs's `p2p status`.
//!
//! `car_received_already_held` leaves out the root a fetch asks for by name,
//! which gossip delivered first: how many heads gossip delivers depends on
//! when a writer's coalesced announcement lands, so only history sent again
//! is counted, and every scenario expects none.

use std::collections::BTreeMap;

use integration_test::actions::{
    Actions, AddCollection, AddCollectionSubscription, AddDoc, ConnectPeers, DisconnectPeers,
    NewNode, Request, SyncCounters, UpdateDoc, WaitForPeersEvents,
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

/// `db0` and `db1`, networked, both holding `Users`.
fn nodes() -> Vec<Actions> {
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
                sdl: SDL.into(),
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
    actions
}

fn connect() -> Vec<Actions> {
    let mut actions: Vec<Actions> = vec![ConnectPeers {
        target: "db1".into(),
        common: on("db0"),
        ..Default::default()
    }
    .into()];
    for node in ["db0", "db1"] {
        actions.push(
            AddCollectionSubscription {
                collections: vec!["Users".into()],
                common: on(node),
                ..Default::default()
            }
            .into(),
        );
    }
    actions
}

fn reconnect() -> Actions {
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

fn create(node: &str) -> Actions {
    AddDoc {
        collection: "Users".into(),
        doc_map: Some(json!({ "name": "n0", "age": 0 })),
        common: Common {
            retry: true,
            capture: captures([("doc", "/docID")]),
            ..on(node)
        },
        ..Default::default()
    }
    .into()
}

fn update(node: &str, fields: Value) -> Actions {
    UpdateDoc {
        collection: "Users".into(),
        doc_id: Text::reference("doc"),
        doc_map: fields,
        expected_data: json!({}),
        common: Common {
            retry: true,
            ..on(node)
        },
        ..Default::default()
    }
    .into()
}

/// `count` updates on `node`, alternating between both fields and the name
/// alone, the last setting the name to `n{count}`.
fn updates(node: &str, count: u32) -> Vec<Actions> {
    (1..=count)
        .map(|idx| match idx % 2 {
            0 => update(node, json!({ "name": format!("n{idx}"), "age": idx })),
            _ => update(node, json!({ "name": format!("n{idx}") })),
        })
        .collect()
}

/// `node` comes to hold the document with `name`.
fn holds(node: &str, name: &str) -> Actions {
    Request {
        request: "query { Users { name } }".into(),
        expected_data: json!({ "Users": [{ "name": name }] }),
        common: Common {
            eventually: Some(Eventually::default()),
            ..on(node)
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

fn at_most(node: &str, limits: &[(&str, u64)]) -> Actions {
    SyncCounters {
        at_most: counters(limits),
        common: on(node),
        ..Default::default()
    }
    .into()
}

/// `node` served at least one CAR block, and at most `limit`.
fn car_served_within(node: &str, limit: u64) -> Actions {
    SyncCounters {
        at_most: counters(&[("car_served_cids", limit)]),
        at_least: counters(&[("car_served_cids", 1)]),
        common: on(node),
        ..Default::default()
    }
    .into()
}

/// Captures `node`'s `car_served_cids` as `name`.
fn served_so_far(node: &str, name: &str) -> Actions {
    SyncCounters {
        common: Common {
            capture: captures([(name, "/car_served_cids")]),
            ..on(node)
        },
        ..Default::default()
    }
    .into()
}

/// Since the capture `name`, `node` served at least one CAR block, and at
/// most `limit`.
fn car_served_since(node: &str, name: &str, limit: u64) -> Actions {
    let since = [("car_served_cids".to_owned(), name.to_owned())].into();
    SyncCounters {
        at_most: counters(&[("car_served_cids", limit)]),
        at_least: counters(&[("car_served_cids", 1)]),
        since,
        common: on(node),
        ..Default::default()
    }
    .into()
}

fn case(id: &'static str, actions: Vec<Actions>) {
    TestCase {
        id,
        actions: nodes().into_iter().chain(actions).map(Into::into).collect(),
        client_types: Some(&[ClientType::InProcess, ClientType::RustHttp, ClientType::Cli]),
        ..Default::default()
    }
    .execute();
}

/// Steady state: `db1` holds the history and is told of each new head in
/// turn. Every update is at most three blocks (a composite and two fields),
/// so `db0` serves at most that per update, however long the history grows,
/// and `db1` is sent no block it already holds.
#[test]
#[mantra_macros::req_test("defra.net.sync.exact_fetch.steady_state")]
fn each_new_head_over_held_history_is_served_alone() {
    const UPDATES: u32 = 30;
    let mut actions = connect();
    actions.push(create("db0"));
    actions.push(holds("db1", "n0"));
    for (idx, step) in updates("db0", UPDATES).into_iter().enumerate() {
        actions.push(step);
        actions.push(holds("db1", &format!("n{}", idx + 1)));
    }
    actions.push(car_served_within("db0", u64::from(3 * (UPDATES + 1))));
    actions.push(at_most("db1", &[("car_received_already_held", 0)]));
    case(integration_test::scenario_id!(), actions);
}

/// `db0` makes a branch 40 updates deep while `db1`, cut off, makes an update
/// of its own. Back in touch, `db1` is served the 41 missing updates and
/// none of the history both share, in spite of the branch being far deeper
/// than any depth cut. Fetching `db1`'s update the other way, `db0` is
/// served that update alone (three blocks at most), though none of `db0`'s
/// own recent heads are on `db1`.
#[test]
#[mantra_macros::req_test("defra.net.sync.exact_fetch.concurrent_branch")]
fn a_deep_concurrent_branch_is_served_without_shared_history() {
    const HISTORY: u32 = 20;
    const BRANCH: u32 = 40;
    let mut actions = connect();
    actions.push(create("db0"));
    actions.extend(updates("db0", HISTORY));
    actions.push(holds("db1", &format!("n{HISTORY}")));
    actions.push(disconnect());
    actions.extend(
        (1..=BRANCH).map(|idx| update("db0", json!({ "name": format!("b{idx}"), "age": idx }))),
    );
    actions.push(update("db1", json!({ "age": 999 })));
    actions.push(served_so_far("db0", "served_before_branch"));
    actions.push(reconnect());
    actions.push(update("db0", json!({ "name": "last" })));
    actions.push(holds("db1", "last"));
    actions.push(car_served_since(
        "db0",
        "served_before_branch",
        u64::from(3 * (BRANCH + 1)),
    ));
    actions.push(at_most(
        "db1",
        &[("car_received_already_held", 0), ("car_served_cids", 3)],
    ));
    actions.push(at_most("db0", &[("car_received_already_held", 0)]));
    case(integration_test::scenario_id!(), actions);
}

/// `db1` holds none of a document with a long history: it is served the
/// whole DAG and converges, sent no block it already holds.
#[test]
#[mantra_macros::req_test("defra.net.sync.exact_fetch.empty_receiver")]
fn a_receiver_holding_nothing_still_gets_the_whole_document() {
    let mut actions = vec![create("db0")];
    actions.extend(updates("db0", 30));
    actions.extend(connect());
    actions.push(update("db0", json!({ "name": "last" })));
    actions.push(holds("db1", "last"));
    actions.push(at_most("db1", &[("car_received_already_held", 0)]));
    case(integration_test::scenario_id!(), actions);
}
