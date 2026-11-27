//! In a collection with a policy, `_documentUpdates` hands a reader only the
//! rows of documents it may read and that are not deleted, and moves its
//! cursor past the others (`defra.events.delivery.visibility`).

use integration_test::actions::{
    Actions, AddCollection, AddDACPolicy, AddDoc, DeleteDoc, UpdateDoc,
};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use integration_test::ClientType;
use serde_json::json;

use super::updates;

pub(super) const POLICY: &str = r#"
description: a policy that makes the collection's documents private
name: test
resources:
- name: users
  permissions:
  - name: delete
  - expr: reader
    name: read
  - name: update
  relations:
  - manages:
    - reader
    name: admin
    types:
    - actor
  - name: reader
    types:
    - actor
"#;

pub(super) const OWNER: u64 = 1;
pub(super) const STRANGER: u64 = 2;

pub(super) fn as_identity(seed: u64) -> Common {
    Common {
        identity_seed: Some(seed),
        retry: true,
        ..Default::default()
    }
}

pub(super) fn collection(policy: bool) -> Vec<Actions> {
    if !policy {
        return vec![AddCollection {
            sdl: "type Users { name: String }".into(),
            ..Default::default()
        }
        .into()];
    }
    vec![
        AddDACPolicy {
            policy: POLICY.into(),
            expected_data: json!({ "policyID": m::nonempty_string() }),
            common: Common {
                identity_seed: Some(OWNER),
                capture: captures([("policy", "/policyID")]),
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddCollection {
            sdl: Text::concat([
                Text::from(r#"type Users @policy(id: ""#),
                Text::reference("policy"),
                Text::from(r#"", resource: "users") { name: String }"#),
            ]),
            ..Default::default()
        }
        .into(),
    ]
}

/// The owner creates `name` and updates it once: one journal row.
fn updated(name: &str, capture: &str) -> [Actions; 2] {
    [
        AddDoc {
            collection: "Users".into(),
            doc_map: Some(json!({ "name": name })),
            common: Common {
                capture: captures([(capture, "/docID")]),
                ..as_identity(OWNER)
            },
            ..Default::default()
        }
        .into(),
        UpdateDoc {
            collection: "Users".into(),
            doc_id: Text::reference(capture),
            doc_map: json!({ "name": format!("{name} 2") }),
            common: as_identity(OWNER),
            ..Default::default()
        }
        .into(),
    ]
}

pub(super) fn delete(capture: &str) -> Actions {
    DeleteDoc {
        collection: "Users".into(),
        doc_id: Text::reference(capture),
        common: as_identity(OWNER),
        ..Default::default()
    }
    .into()
}

/// The page from cursor 0 as `seed` reads it: `head` and `next` are both 2,
/// whatever is shown.
fn page(seed: u64, entries: serde_json::Value) -> Actions {
    updates(
        "Users",
        Text::from("0"),
        "head next entries { cursor docID }",
        None,
        json!({ "_documentUpdates": { "head": "2", "next": "2", "entries": entries } }),
        Common {
            identity_seed: Some(seed),
            ..Default::default()
        },
    )
    .into()
}

pub(super) fn case(actions: Vec<Actions>) -> TestCase {
    let mut case = TestCase::from_actions(actions);
    // `_documentUpdates` is defradb.rs only.
    case.client_types = Some(&[ClientType::InProcess, ClientType::RustHttp, ClientType::Cli]);
    case
}

#[test]
#[mantra_macros::req_test("defra.events.delivery.visibility")]
fn hidden_rows_are_passed_and_not_shown() {
    let mut actions = collection(true);
    actions.extend(updated("Ann", "ann"));
    actions.extend(updated("Bob", "bob"));
    actions.push(page(
        OWNER,
        json!([
            { "cursor": "1", "docID": m::r("ann") },
            { "cursor": "2", "docID": m::r("bob") },
        ]),
    ));
    actions.push(page(STRANGER, json!([])));
    actions.push(delete("ann"));
    actions.push(page(
        OWNER,
        json!([{ "cursor": "2", "docID": m::r("bob") }]),
    ));
    actions.push(page(STRANGER, json!([])));
    let mut case = case(actions);
    case.id = integration_test::scenario_id!();
    case.execute();
}

#[test]
#[mantra_macros::req_test("defra.events.delivery.visibility")]
fn a_collection_without_a_policy_shows_a_deleted_documents_rows() {
    let mut actions = collection(false);
    actions.extend(updated("Ann", "ann"));
    actions.extend(updated("Bob", "bob"));
    actions.push(delete("ann"));
    let all = json!([
        { "cursor": "1", "docID": m::r("ann") },
        { "cursor": "2", "docID": m::r("bob") },
    ]);
    actions.push(page(OWNER, all.clone()));
    actions.push(page(STRANGER, all));
    let mut case = case(actions);
    case.id = integration_test::scenario_id!();
    case.execute();
}
