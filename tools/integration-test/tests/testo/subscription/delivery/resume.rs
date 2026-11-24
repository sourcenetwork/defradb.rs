//! A reader that saved its cursor before the node reopened reads exactly the
//! rows committed after it, in commit order (`defra.events.delivery.resume`,
//! `defra.events.delivery.order`).

use integration_test::actions::{AddCollection, AddDoc, NewNode, Restart, UpdateDoc};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use integration_test::ClientType;
use serde_json::json;

use super::updates;

fn add(name: &str, capture: &str) -> AddDoc {
    AddDoc {
        collection: "User".into(),
        doc_map: Some(json!({ "name": name })),
        common: Common {
            retry: true,
            capture: captures([(capture, "/docID")]),
            ..Default::default()
        },
        ..Default::default()
    }
}

fn update(doc: &str, name: &str) -> UpdateDoc {
    UpdateDoc {
        collection: "User".into(),
        doc_id: Text::reference(doc),
        doc_map: json!({ "name": name }),
        common: Common {
            retry: true,
            ..Default::default()
        },
        ..Default::default()
    }
}

#[test]
#[mantra_macros::req_test("defra.events.delivery.resume")]
#[mantra_macros::req_test("defra.events.delivery.order")]
fn a_saved_cursor_resumes_after_a_reopen_in_commit_order() {
    let mut case = TestCase::from_actions(vec![
        NewNode {
            database_dir: Some("".into()),
            ..Default::default()
        }
        .into(),
        AddCollection {
            sdl: "type User { name: String }".into(),
            ..Default::default()
        }
        .into(),
        add("Ann", "ann").into(),
        add("Bob", "bob").into(),
        update("ann", "Ann 2").into(),
        update("bob", "Bob 2").into(),
        updates(
            "User",
            Text::from("0"),
            "head next entries { cursor docID }",
            None,
            json!({ "_documentUpdates": {
                "head": "2",
                "next": "2",
                "entries": [
                    { "cursor": "1", "docID": m::r("ann") },
                    { "cursor": "2", "docID": m::r("bob") },
                ],
            }}),
            Common {
                capture: captures([("saved", "/data/_documentUpdates/next")]),
                ..Default::default()
            },
        )
        .into(),
        Restart {
            ..Default::default()
        }
        .into(),
        add("Cy", "cy").into(),
        update("cy", "Cy 2").into(),
        update("ann", "Ann 3").into(),
        update("bob", "Bob 3").into(),
        updates(
            "User",
            Text::reference("saved"),
            "head next entries { cursor docID }",
            None,
            json!({ "_documentUpdates": {
                "head": "5",
                "next": "5",
                "entries": [
                    { "cursor": "3", "docID": m::r("cy") },
                    { "cursor": "4", "docID": m::r("ann") },
                    { "cursor": "5", "docID": m::r("bob") },
                ],
            }}),
            Common::default(),
        )
        .into(),
    ]);
    case.id = integration_test::scenario_id!();
    // `_documentUpdates` is defradb.rs only.
    case.client_types = Some(&[ClientType::InProcess, ClientType::RustHttp, ClientType::Cli]);
    case.execute();
}
