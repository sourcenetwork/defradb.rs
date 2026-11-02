//! Unique-index errors name only the writer (`defra.index.unique_writer`).
//! DocIDs are content-addressed, so the writer's DocID in the text is fixed.

use integration_test::actions::{Actions, AddCollection, AddDoc, UpdateDoc};
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

const VIOLATION: &str = "can not index a doc's field(s) that violates unique index";

fn add(name: &str, age: i64, capture: Option<&str>, error: Option<String>) -> Actions {
    AddDoc {
        collection: "User".into(),
        doc_map: Some(json!({ "name": name, "age": age })),
        expected_error: error,
        common: Common {
            capture: capture.map_or_else(Default::default, |c| captures([(c, "/docID")])),
            ..Default::default()
        },
        ..Default::default()
    }
    .into()
}

#[test]
#[mantra_macros::req_test("defra.index.unique_writer")]
fn a_unique_index_error_names_the_writer_and_its_value() {
    TestCase::from_actions(vec![
        AddCollection {
            sdl: "type User { name: String @index(unique: true) age: Int }".into(),
            ..Default::default()
        }
        .into(),
        add("alice", 1, Some("holder"), None),
        add(
            "alice",
            2,
            None,
            Some(format!("{VIOLATION}. DocID: {ALICE_2}, name: alice")),
        ),
        add("bob", 3, Some("bob"), None),
        UpdateDoc {
            collection: "User".into(),
            doc_id: Text::reference("bob"),
            doc_map: json!({ "name": "alice" }),
            expected_error: Some(format!(
                "failed to update index: {VIOLATION}. DocID: {BOB_3}, name: alice. IndexName: User_name_ASC"
            )),
            ..Default::default()
        }
        .into(),
    ])
    .execute();
}

/// The DocID of `{"name":"alice","age":2}`, the writer.
const ALICE_2: &str = "bae-0dd8fc64-0dc2-54f5-b156-52e560dff6f3";
/// The DocID of `{"name":"bob","age":3}`, the writer of the update.
const BOB_3: &str = "bae-f2895d11-b901-58b7-843c-61b2ecc65816";
