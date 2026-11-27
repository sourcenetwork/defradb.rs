//! An explicit document sync returns once every document it asked for has
//! merged, however many times each merge is reported (#3468), and a
//! document it already holds does not keep it waiting.

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

/// The six documents, created on the sender.
fn sender_docs() -> Vec<Actions> {
    (0..DOCS)
        .map(|i| {
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
            .into()
        })
        .collect()
}

/// The receiver asks for the documents `docs`, and the call must return
/// within `within_millis` when it is set.
fn sync(docs: impl Iterator<Item = usize>, within_millis: Option<u64>) -> Actions {
    SyncDocs {
        collection: "Users".into(),
        doc_ids: docs.map(|i| Text::reference(&doc_ref(i))).collect(),
        concurrency: 1,
        within_millis,
        common: on(RECEIVER),
        ..Default::default()
    }
    .into()
}

/// A query on the receiver, right after the sync, returns all six documents.
fn all_docs_on_receiver() -> Actions {
    let users: Vec<Value> = (0..DOCS)
        .map(|i| json!({ "name": format!("user{i}") }))
        .collect();
    Request {
        request: "query { Users(order: {name: ASC}) { name } }".into(),
        expected_data: json!({ "Users": users }),
        common: on(RECEIVER),
        ..Default::default()
    }
    .into()
}

#[test]
#[mantra_macros::req_test("defra.net.sync.documents_complete")]
fn a_document_sync_returns_once_every_asked_document_merged() {
    let mut actions = connected_iroh_peers();
    actions.extend(sender_docs());
    actions.push(sync(0..DOCS, None));
    actions.push(all_docs_on_receiver());
    TestCase::from_actions(actions).execute();
}

/// The receiver holds `doc0` from a first sync, then asks for all six. The
/// peer's answer for `doc0` names a head already merged, so nothing merges
/// for it; the sync still returns once the other five merge, before its 3 s
/// idle timeout would end the wait.
#[test]
#[mantra_macros::req_test("defra.net.sync.documents_already_held")]
fn a_document_already_held_does_not_hold_up_the_sync() {
    let mut actions = connected_iroh_peers();
    actions.extend(sender_docs());
    actions.push(sync(0..1, None));
    actions.push(sync(0..DOCS, Some(2_500)));
    actions.push(all_docs_on_receiver());
    TestCase::from_actions(actions).execute();
}
