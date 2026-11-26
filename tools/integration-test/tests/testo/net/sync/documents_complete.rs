//! An explicit document sync returns once every document it asked for has
//! merged, however many times each merge is reported (#3468).

use integration_test::actions::{
    Actions, AddCollection, AddDoc, ConnectPeers, NewNode, Request, SyncDocs,
};
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::{json, Value};

const RECEIVER: &str = "db0";
const SENDER: &str = "db1";
const DOCS: usize = 6;

fn on(node: &str) -> Common {
    Common {
        node: Some(node.into()),
        ..Default::default()
    }
}

fn doc_ref(i: usize) -> String {
    format!("doc{i}")
}

/// The receiver and sender on iroh, whose document sync waits on merge
/// reports, connected but not subscribed, so only the sync brings documents.
fn connected_iroh_peers() -> Vec<Actions> {
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
        actions.push(
            AddCollection {
                sdl: "type Users { name: String }".into(),
                common: on(node),
                ..Default::default()
            }
            .into(),
        );
    }
    actions.push(
        ConnectPeers {
            target: SENDER.into(),
            common: on(RECEIVER),
            ..Default::default()
        }
        .into(),
    );
    actions
}

#[test]
#[mantra_macros::req_test("defra.net.sync.documents_complete")]
fn a_document_sync_returns_once_every_asked_document_merged() {
    let mut actions = connected_iroh_peers();
    for i in 0..DOCS {
        actions.push(
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": format!("user{i}") })),
                common: Common {
                    node: Some(SENDER.into()),
                    capture: captures([(doc_ref(i).as_str(), "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
        );
    }
    actions.push(
        SyncDocs {
            collection: "Users".into(),
            doc_ids: (0..DOCS).map(|i| Text::reference(&doc_ref(i))).collect(),
            concurrency: 1,
            common: on(RECEIVER),
            ..Default::default()
        }
        .into(),
    );
    let users: Vec<Value> = (0..DOCS)
        .map(|i| json!({ "name": format!("user{i}") }))
        .collect();
    actions.push(
        Request {
            request: "query { Users(order: {name: ASC}) { name } }".into(),
            expected_data: json!({ "Users": users }),
            common: on(RECEIVER),
            ..Default::default()
        }
        .into(),
    );
    TestCase::from_actions(actions).execute();
}
