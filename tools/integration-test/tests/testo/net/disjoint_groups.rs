//! Two groups of connected nodes, with no connection between them, never hear
//! each other's document updates, whatever infrastructure they share.

use integration_test::actions::{
    Actions, AddCollection, AddDoc, AddDocumentSubscription, ConnectPeers, NewNode, Request, UpdateDoc, Wait,
    WaitForPeersEvents, WaitForSync,
};
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;
use testo::action::ActionInstance;

const NODES: [&str; 4] = ["a1", "a2", "b1", "b2"];

fn on(node: &str) -> Common {
    Common {
        node: Some(node.into()),
        ..Default::default()
    }
}

/// `writer` updates the shared doc's `Age` and `peer`, its one connection,
/// merges that update.
fn update_reaches_peer(writer: &str, peer: &str, age: i64, head: &str) -> Vec<ActionInstance<Actions>> {
    vec![
        UpdateDoc {
            collection: "Users".into(),
            doc_id: Text::reference("doc"),
            doc_map: json!({ "Age": age }),
            expected_data: json!({}),
            common: Common { retry: true, ..on(writer) },
            ..Default::default()
        }
        .into(),
        WaitForSync {
            doc_id: Some(Text::reference("doc")),
            common: Common { capture: captures([(head, "/cid")]), ..on(writer) },
            ..Default::default()
        }
        .into(),
        WaitForSync {
            doc_id: Some(Text::reference("doc")),
            cid: Some(Text::reference(head)),
            common: on(peer),
            ..Default::default()
        }
        .into(),
    ]
}

#[test]
#[mantra_macros::req_test("go.net.peer.isolation")]
fn p2p_update_does_not_cross_to_an_unconnected_group() {
    let mut actions: Vec<ActionInstance<Actions>> = Vec::new();
    for node in NODES {
        actions.push(NewNode { network: true, common: on(node), ..Default::default() }.into());
        actions.push(
            AddCollection {
                sdl: "type Users { Name: String Age: Int }".into(),
                common: on(node),
                ..Default::default()
            }
            .into(),
        );
        actions.push(WaitForPeersEvents { start: true, common: on(node), ..Default::default() }.into());
    }
    // The same content on every node gives every node the same doc.
    for node in NODES {
        actions.push(
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "Shahzad", "Age": 300 })),
                common: Common {
                    retry: true,
                    capture: if node == "a1" { captures([("doc", "/docID")]) } else { Default::default() },
                    ..on(node)
                },
                ..Default::default()
            }
            .into(),
        );
        actions.push(
            WaitForSync { doc_id: Some(Text::reference("doc")), common: on(node), ..Default::default() }
                .into(),
        );
    }
    actions.push(ConnectPeers { target: "a1".into(), common: on("a2"), ..Default::default() }.into());
    actions.push(ConnectPeers { target: "b1".into(), common: on("b2"), ..Default::default() }.into());
    for node in NODES {
        actions.push(
            AddDocumentSubscription { doc_ids: vec![Text::reference("doc")], common: on(node), ..Default::default() }
                .into(),
        );
    }
    actions.extend(update_reaches_peer("a1", "a2", 60, "head_a"));
    actions.extend(update_reaches_peer("b1", "b2", 70, "head_b"));
    // Time for a leaked update to arrive before the groups are read.
    actions.push(Wait { millis: Some(2000), ..Default::default() }.into());
    for (node, age) in [("a1", 60), ("a2", 60), ("b1", 70), ("b2", 70)] {
        actions.push(
            Request {
                request: "query { Users { Age } }".into(),
                expected_data: json!({ "Users": [{ "Age": age }] }),
                common: on(node),
                ..Default::default()
            }
            .into(),
        );
    }
    TestCase {
        id: integration_test::scenario_id!(),
        actions,
        without_db: true,
        ..Default::default()
    }
    .execute();
}
