//! How long the update journal keeps its rows, and what a reader holding a
//! cursor whose rows are gone is told (`defra.events.delivery.retention`,
//! `defra.events.delivery.unavailable`).

use integration_test::actions::shared::Filter;
use integration_test::actions::{Actions, AddCollection, AddDoc, Request, Truncate, UpdateDoc};
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

fn unavailable(after: &str, oldest: &str) -> Request {
    Request {
        request: format!(
            r#"query {{ _documentUpdates(collection: "User", after: "{after}") {{ head }} }}"#
        )
        .into(),
        expected_error: Some(format!(
            "update cursor is no longer available on this node: resync the collection and read after cursor {oldest}"
        )),
        ..Default::default()
    }
}

/// Ann and Bob, each updated once: rows "1" for Ann and "2" for Bob.
fn two_updates() -> Vec<Actions> {
    vec![
        AddCollection {
            sdl: "type User { name: String }".into(),
            ..Default::default()
        }
        .into(),
        add("Ann", "ann").into(),
        add("Bob", "bob").into(),
        update("ann", "Ann 2").into(),
        update("bob", "Bob 2").into(),
    ]
}

#[test]
#[mantra_macros::req_test("defra.events.delivery.retention")]
#[mantra_macros::req_test("defra.events.delivery.unavailable")]
fn a_cursor_before_a_full_truncate_is_no_longer_available() {
    let mut actions = two_updates();
    actions.extend([
        Truncate {
            collection: "User".into(),
            ..Default::default()
        }
        .into(),
        unavailable("0", "2").into(),
        unavailable("1", "2").into(),
        updates(
            "User",
            Text::from("2"),
            "head next entries { cursor docID }",
            None,
            json!({ "_documentUpdates": { "head": "2", "next": "2", "entries": [] } }),
            Common::default(),
        )
        .into(),
        add("Cy", "cy").into(),
        update("cy", "Cy 2").into(),
        updates(
            "User",
            Text::from("2"),
            "head next entries { cursor docID }",
            None,
            json!({ "_documentUpdates": {
                "head": "3",
                "next": "3",
                "entries": [{ "cursor": "3", "docID": m::r("cy") }],
            }}),
            Common::default(),
        )
        .into(),
        unavailable("1", "2").into(),
    ]);
    let mut case = TestCase::from_actions(actions);
    case.id = integration_test::scenario_id!();
    // `_documentUpdates` is defradb.rs only.
    case.client_types = Some(&[ClientType::InProcess, ClientType::RustHttp, ClientType::Cli]);
    case.execute();
}

#[test]
#[mantra_macros::req_test("defra.events.delivery.retention")]
fn a_filtered_truncate_drops_no_row() {
    let mut actions = two_updates();
    actions.extend([
        Truncate {
            collection: "User".into(),
            filter: Some(Filter {
                kind: "map".into(),
                value: Some(json!({ "_docID": { "_in": [m::r("ann")] } })),
            }),
            ..Default::default()
        }
        .into(),
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
            Common::default(),
        )
        .into(),
    ]);
    let mut case = TestCase::from_actions(actions);
    case.id = integration_test::scenario_id!();
    // `_documentUpdates` is defradb.rs only.
    case.client_types = Some(&[ClientType::InProcess, ClientType::RustHttp, ClientType::Cli]);
    case.execute();
}
