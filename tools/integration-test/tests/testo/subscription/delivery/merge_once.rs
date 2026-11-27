//! An update a peer delivers appears exactly once in the receiving node's
//! `_documentUpdates`, naming the block the sender committed
//! (`defra.events.delivery.merge-once`).

use integration_test::actions::{
    Actions, AddCollection, AddDoc, AddReplicator, NewNode, UpdateDoc, WaitForPeersEvents,
    WaitForSync,
};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use integration_test::ClientType;
use serde_json::json;

use super::updates;

fn on(node: &str) -> Common {
    Common {
        node: Some(node.into()),
        ..Default::default()
    }
}

fn synced(cid: &str) -> [Actions; 2] {
    [
        WaitForSync {
            doc_id: Some(Text::reference("doc")),
            common: Common {
                capture: captures([(cid, "/cid")]),
                ..on("db0")
            },
            ..Default::default()
        }
        .into(),
        WaitForSync {
            doc_id: Some(Text::reference("doc")),
            cid: Some(Text::reference(cid)),
            expected_data: json!({}),
            common: on("db1"),
            ..Default::default()
        }
        .into(),
    ]
}

fn journal() -> Actions {
    updates(
        "User",
        Text::from("0"),
        "head entries { docID cid }",
        Some("db1"),
        json!({ "_documentUpdates": {
            "head": "1",
            "entries": [{ "docID": m::r("doc"), "cid": m::r("updated") }],
        }}),
        Common::default(),
    )
    .into()
}

#[test]
#[mantra_macros::req_test("defra.events.delivery.merge-once")]
fn a_replicated_update_appears_once() {
    let mut actions: Vec<Actions> = Vec::new();
    for node in ["db0", "db1"] {
        actions.push(
            NewNode {
                network: true,
                database_dir: Some("".into()),
                common: on(node),
                ..Default::default()
            }
            .into(),
        );
        actions.push(
            AddCollection {
                sdl: "type User { name: String }".into(),
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
    actions.push(
        AddReplicator {
            target: "db1".into(),
            common: on("db0"),
            ..Default::default()
        }
        .into(),
    );
    actions.push(
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "Ann" })),
            common: Common {
                retry: true,
                capture: captures([("doc", "/docID")]),
                ..on("db0")
            },
            ..Default::default()
        }
        .into(),
    );
    actions.extend(synced("created"));
    actions.push(
        UpdateDoc {
            collection: "User".into(),
            doc_id: Text::reference("doc"),
            doc_map: json!({ "name": "Ann 2" }),
            common: Common {
                retry: true,
                ..on("db0")
            },
            ..Default::default()
        }
        .into(),
    );
    actions.extend(synced("updated"));
    actions.push(journal());
    let mut case = TestCase::from_actions(actions);
    case.id = integration_test::scenario_id!();
    case.without_db = true;
    // `_documentUpdates` is defradb.rs only.
    case.client_types = Some(&[ClientType::InProcess, ClientType::RustHttp, ClientType::Cli]);
    case.execute();
}
