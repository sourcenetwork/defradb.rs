//! A patch may not remove a field an index reads: the index would stay and
//! fail every later write to the collection.

use integration_test::actions::{AddCollection, AddDoc, PatchCollection, Request};
use integration_test::test_case::TestCase;
use integration_test::ClientType;
use serde_json::json;

#[test]
#[mantra_macros::req_test("defra.index.field_removal")]
fn removing_an_indexed_field_is_refused_and_writes_go_on() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: "type Users { name: String @index  age: Int }".into(),
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"[{ "op": "remove", "path": "/Users/Fields/2" }]"#.into(),
                expected_error: Some(
                    "cannot remove field 'name': index 'Users_name_ASC' depends on it, drop the index first"
                        .into(),
                ),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "John", "age": 21 })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"query { Users(filter: {name: {_eq: "John"}}) { name age } }"#.into(),
                expected_data: json!({ "Users": [{ "name": "John", "age": 21 }] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.index.field_removal")]
fn removing_a_field_no_index_reads_is_accepted() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: "type Users { name: String @index  points: Int }".into(),
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"[{ "op": "remove", "path": "/Users/Fields/2" }]"#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "John" })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"query { Users(filter: {name: {_eq: "John"}}) { name } }"#.into(),
                expected_data: json!({ "Users": [{ "name": "John" }] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.index.field_removal")]
fn removing_a_fulltext_field_names_the_field_and_keeps_the_collection_usable() {
    TestCase {
        id: integration_test::scenario_id!(),
        // Go's pinned schema parser does not support @fulltext.
        client_types: Some(&[ClientType::InProcess, ClientType::RustHttp, ClientType::Cli]),
        actions: vec![
            AddCollection {
                sdl: "type Notes { body: String @fulltext }".into(),
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"[{ "op": "remove", "path": "/Notes/Fields/1" }]"#.into(),
                expected_error: Some(
                    "cannot remove field 'body': a full-text index depends on it, drop the index first. Collection: Notes"
                        .into(),
                ),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Notes".into(),
                doc_map: Some(json!({ "body": "still writable" })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"query { Notes { body } }"#.into(),
                expected_data: json!({ "Notes": [{ "body": "still writable" }] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
