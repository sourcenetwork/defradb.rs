//! A write whose head block is too large for one gossip broadcast still
//! replicates to a subscribed peer over iroh (#2820).

use integration_test::actions::shared::{EventType, Topic};
use integration_test::actions::{
    Actions, AddCollection, AddCollectionSubscription, ConnectPeers, GetCollections, NewNode,
    Request, WaitForPeersEvents,
};
use integration_test::scenario::{captures, Common, Eventually, Text};
use integration_test::test_case::TestCase;
use serde_json::{json, Map, Value};

const RECEIVER: &str = "db0";
const SENDER: &str = "db1";

/// Fields in the collection. A write to every one links all of them from its
/// composite block, the block gossip carries, which then holds well over
/// iroh-gossip's 4,023-byte broadcast budget.
const FIELDS: usize = 160;

fn field(i: usize) -> String {
    format!("field_number_{i:03}")
}

fn sdl() -> String {
    let fields: Vec<String> = (0..FIELDS).map(|i| format!("{}: Int", field(i))).collect();
    format!("type Wide {{ {} }}", fields.join(" "))
}

/// Every field set to its number plus `base`.
fn input(base: usize) -> String {
    let values: Vec<String> = (0..FIELDS)
        .map(|i| format!("{}: {}", field(i), base + i))
        .collect();
    format!("{{{}}}", values.join(", "))
}

fn on(node: &str) -> Common {
    Common {
        node: Some(node.into()),
        ..Default::default()
    }
}

/// Two iroh nodes subscribed to Wide and joined on its topic.
fn subscribed_peers() -> Vec<Actions> {
    let mut actions: Vec<Actions> = Vec::new();
    for node in [RECEIVER, SENDER] {
        actions.push(
            NewNode {
                network: true,
                iroh: true,
                common: on(node),
                ..Default::default()
            }
            .into(),
        );
    }
    for node in [RECEIVER, SENDER] {
        actions.push(
            AddCollection {
                sdl: sdl().into(),
                common: on(node),
                ..Default::default()
            }
            .into(),
        );
        actions.push(
            AddCollectionSubscription {
                collections: vec!["Wide".into()],
                common: on(node),
                ..Default::default()
            }
            .into(),
        );
    }
    actions.extend([
        GetCollections {
            collection_name: Some("Wide".into()),
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
    ]);
    actions
}

fn on_sender(request: String) -> Actions {
    Request {
        request: request.into(),
        common: on(SENDER),
        ..Default::default()
    }
    .into()
}

/// The receiver's first and last fields, waited for until each holds its
/// number plus `base`.
fn receiver_holds(base: usize) -> Actions {
    let (first, last) = (field(0), field(FIELDS - 1));
    let mut doc = Map::new();
    doc.insert(first.clone(), json!(base));
    doc.insert(last.clone(), json!(base + FIELDS - 1));
    Request {
        request: format!("query {{ Wide {{ {first} {last} }} }}").into(),
        expected_data: json!({ "Wide": [Value::Object(doc)] }),
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
    .into()
}

#[test]
#[mantra_macros::req_test("defra.p2p.iroh_large_update")]
fn a_document_too_large_for_gossip_replicates_over_iroh() {
    let mut actions = subscribed_peers();
    actions.push(on_sender(format!(
        "mutation {{ add_Wide(input: [{}]) {{ _docID }} }}",
        input(0)
    )));
    actions.push(receiver_holds(0));
    TestCase::from_actions(actions).execute();
}

#[test]
#[mantra_macros::req_test("defra.p2p.iroh_large_update")]
fn an_update_too_large_for_gossip_replicates_over_iroh() {
    let mut actions = subscribed_peers();
    actions.push(on_sender(format!(
        "mutation {{ add_Wide(input: [{}]) {{ _docID }} }}",
        input(0)
    )));
    actions.push(receiver_holds(0));
    actions.push(on_sender(format!(
        "mutation {{ update_Wide(input: {}) {{ _docID }} }}",
        input(1000)
    )));
    actions.push(receiver_holds(1000));
    TestCase::from_actions(actions).execute();
}
