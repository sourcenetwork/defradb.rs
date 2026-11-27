//! An upsert asks the node's write validator in both of its arms: a refusal
//! fails the upsert with the validator's message and writes nothing, and an
//! upsert the validator lets through runs (`defra.write_validator`).

use integration_test::actions::shared::RefuseWrites;
use integration_test::actions::{Actions, AddCollection, NewNode, Request};
use integration_test::test_case::TestCase;
use serde_json::{json, Value};

const REFUSED: &str = "a note's body may not be forbidden";

/// A node whose write validator refuses any write setting `body` to
/// `forbidden`, with a `Note` collection.
fn validated_notes() -> Vec<Actions> {
    vec![
        NewNode {
            network: true,
            iroh: true,
            write_validator: Some(RefuseWrites {
                field: "body".into(),
                value: json!("forbidden"),
                message: REFUSED.into(),
            }),
            ..Default::default()
        }
        .into(),
        AddCollection {
            sdl: "type Note { title: String body: String }".into(),
            ..Default::default()
        }
        .into(),
    ]
}

fn upsert(add_body: &str, update_body: &str, error: Option<&str>, data: Value) -> Actions {
    Request {
        request: format!(
            r#"mutation {{
                upsert_Note(
                    filter: {{title: {{_eq: "first"}}}},
                    add: {{title: "first", body: "{add_body}"}},
                    update: {{body: "{update_body}"}}
                ) {{
                    title
                    body
                }}
            }}"#
        )
        .into(),
        expected_error: error.map(Into::into),
        expected_data: data,
        ..Default::default()
    }
    .into()
}

fn notes(expected: Value) -> Actions {
    Request {
        request: "query { Note { title body } }".into(),
        expected_data: json!({ "Note": expected }),
        ..Default::default()
    }
    .into()
}

/// No note matches, so the upsert would add one; the validator refuses its
/// add input and no note exists after. The same upsert with an allowed body
/// adds the note.
#[test]
#[mantra_macros::req_test("defra.write_validator")]
fn a_write_validator_refuses_an_upsert_that_would_add() {
    let mut actions = validated_notes();
    actions.extend([
        upsert("forbidden", "kept", Some(REFUSED), Value::Null),
        notes(json!([])),
        upsert(
            "allowed",
            "kept",
            None,
            json!({ "upsert_Note": [{ "title": "first", "body": "allowed" }] }),
        ),
        notes(json!([{ "title": "first", "body": "allowed" }])),
    ]);
    TestCase {
        id: integration_test::scenario_id!(),
        actions: actions.into_iter().map(Into::into).collect(),
        ..Default::default()
    }
    .execute();
}

/// A note matches, so the upsert would update it; the validator refuses its
/// update input and the note keeps its body. The same upsert with an allowed
/// body updates the note.
#[test]
#[mantra_macros::req_test("defra.write_validator")]
fn a_write_validator_refuses_an_upsert_that_would_update() {
    let mut actions = validated_notes();
    actions.extend([
        Request {
            request:
                r#"mutation { add_Note(input: {title: "first", body: "original"}) { title } }"#
                    .into(),
            expected_data: json!({ "add_Note": [{ "title": "first" }] }),
            ..Default::default()
        }
        .into(),
        upsert("allowed", "forbidden", Some(REFUSED), Value::Null),
        notes(json!([{ "title": "first", "body": "original" }])),
        upsert(
            "allowed",
            "changed",
            None,
            json!({ "upsert_Note": [{ "title": "first", "body": "changed" }] }),
        ),
        notes(json!([{ "title": "first", "body": "changed" }])),
    ]);
    TestCase {
        id: integration_test::scenario_id!(),
        actions: actions.into_iter().map(Into::into).collect(),
        ..Default::default()
    }
    .execute();
}
